#!/usr/bin/env python3
"""Build a Linux Minecraft candidate with offline rustdoc and source provenance."""
import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parent.parent


def command(*args, env=None):
    return subprocess.check_output(args, cwd=ROOT, env=env, text=True).strip()


def source_status(staging=None):
    args = ['git', 'status', '--porcelain', '--', '.']
    if staging is not None and staging.is_relative_to(ROOT):
        # A custom output directory may be unignored. Only exclude this run's
        # private staging tree; other untracked or modified source still rejects it.
        args.append(f':(top,exclude,literal){staging.relative_to(ROOT).as_posix()}')
    return command(*args)


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def dependency_notices(staging, host, backend):
    features = 'rayengine-minecraft/render'
    if backend == 'wayland':
        features += ',rayengine/wayland'
    meta = json.loads(command('cargo', 'metadata', '--locked', '--format-version', '1',
                              '--filter-platform', host, '--features', features))
    packages = {p['id']: p for p in meta['packages']}
    nodes = {n['id']: n for n in meta['resolve']['nodes']}
    pending = [next(p['id'] for p in meta['packages'] if p['name'] == 'rayengine-minecraft')]
    included = set()
    while pending:
        package = pending.pop()
        if package in included:
            continue
        included.add(package)
        pending.extend(dep['pkg'] for dep in nodes[package]['deps']
                       if any(kind['kind'] is None for kind in dep['dep_kinds']))
    notices = staging / 'THIRD_PARTY_NOTICES'
    notices.mkdir()
    entries = []
    for package in sorted((packages[p] for p in included), key=lambda p: (p['name'], p['version'])):
        root = Path(package['manifest_path']).parent
        license_files = set(p for pattern in ['LICENSE*', 'COPYING*'] for p in root.glob(pattern) if p.is_file())
        if package['source'] is None:
            license_files.add(ROOT / 'LICENSE')
        if package.get('license_file'):
            source = Path(package['license_file'])
            if not source.is_absolute():
                source = root / source
            if source.is_file():
                license_files.add(source)
        if package['name'] == 'raylib-sys':
            license_files.update([root / 'raylib/LICENSE', root / 'raylib/src/external/glfw/LICENSE.md'])
        destination = notices / f"{package['name']}-{package['version']}"
        paths = []
        for source in sorted(license_files):
            relative = source.relative_to(root) if source.is_relative_to(root) else Path(source.name)
            target = destination / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, target)
            paths.append(target.relative_to(staging).as_posix())
        entries.append(dict(name=package['name'], version=package['version'],
                            license=package['license'], repository=package['repository'],
                            authors=package['authors'], license_files=paths))
    (notices / 'metadata.json').write_text(json.dumps(dict(schema_version=1, packages=entries), indent=2) + '\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, default=ROOT / 'artifacts/releases')
    parser.add_argument('--backend', choices=['x11', 'wayland'], default='x11')
    args = parser.parse_args()
    if sys.platform != 'linux':
        parser.error('bundles are currently supported on Linux')
    if source_status():
        parser.error('commit/stash source changes before packaging a candidate')
    meta = json.loads(command('cargo', 'metadata', '--locked', '--no-deps', '--format-version', '1'))
    version = next(p['version'] for p in meta['packages'] if p['name'] == 'rayengine-minecraft')
    compiler = command('rustc', '-Vv')
    host = next(line.split(': ', 1)[1] for line in compiler.splitlines() if line.startswith('host: '))
    commit = command('git', 'rev-parse', 'HEAD')
    epoch = int(command('git', 'show', '-s', '--format=%ct', 'HEAD'))
    name = f'rayengine-minecraft-{version}-{host}-{args.backend}'
    output = args.output.resolve()
    archive = output / f'{name}.tar.gz'
    checksum = archive.with_name(archive.name + '.sha256')
    if archive.exists() or checksum.exists():
        parser.error('bundle already exists; choose a new --output directory')
    output.mkdir(parents=True, exist_ok=True)
    # Keep generated reference pages apart from other projects' cached rustdocs.
    target = Path(meta['target_directory']) / 'minecraft-release'
    env = dict(os.environ, CARGO_TARGET_DIR=str(target))
    features = 'render' if args.backend == 'x11' else 'render,rayengine/wayland'
    subprocess.run(['cargo', 'build', '--locked', '--release', '--target', host,
                    '-p', 'rayengine-minecraft', '--features', features, '--bin', 'minecraft'],
                   cwd=ROOT, env=env, check=True)
    # Clear only this packager's generated reference tree so removed pages cannot
    # leak into a later candidate. Cargo build caches remain available.
    docs = target / host / 'doc'
    if docs.exists():
        shutil.rmtree(docs)
    doc_features = 'rayengine-minecraft/render,rayengine-voxel/render'
    if args.backend == 'wayland':
        doc_features += ',rayengine/wayland'
    doc_env = dict(env, RUSTDOCFLAGS=(env.get('RUSTDOCFLAGS', '') + ' -D warnings').strip())
    subprocess.run(['cargo', 'doc', '--locked', '--workspace', '--no-deps', '--target', host,
                    '--features', doc_features], cwd=ROOT, env=doc_env, check=True)
    binary = target / host / 'release/minecraft'
    libraries = command('ldd', '-v', str(binary))
    with tempfile.TemporaryDirectory(prefix='.minecraft-package-', dir=output) as temp:
        staging = Path(temp) / name
        staging.mkdir()
        shutil.copy2(binary, staging / 'minecraft')
        shutil.copy2(ROOT / 'LICENSE', staging / 'LICENSE')
        shutil.copy2(ROOT / 'examples/minecraft/RELEASE.md', staging / 'README.md')
        shutil.copytree(docs, staging / 'reference')
        dependency_notices(staging, host, args.backend)
        (staging / 'runtime-libraries.txt').write_text(libraries + '\n')
        manifest = dict(schema_version=1, name='rayengine-minecraft', version=version,
                        source_commit=commit, source_epoch=epoch, target=host,
                        backend=args.backend, rustc=compiler, libc=list(platform.libc_ver()),
                        assets='original built-in fallback; imported PNGs are excluded')
        (staging / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
        files = sorted(p for p in staging.rglob('*') if p.is_file())
        (staging / 'SHA256SUMS').write_text(''.join(
            f'{digest(path)}  {path.relative_to(staging).as_posix()}\n' for path in files))
        # Stable order/timestamps/ownership make archive creation independent of
        # the current wall clock. Native build reproducibility is not asserted.
        def normalize(info):
            info.uid = info.gid = 0
            info.uname = info.gname = ''
            info.mtime = epoch
            info.mode = 0o755 if info.isdir() or info.name == f'{name}/minecraft' else 0o644
            return info
        temporary = Path(temp) / archive.name
        with temporary.open('wb') as stream:
            with gzip.GzipFile(filename='', mode='wb', fileobj=stream, mtime=epoch) as compressed:
                with tarfile.open(fileobj=compressed, mode='w') as tar:
                    tar.add(staging, arcname=name, recursive=False, filter=normalize)
                    for path in sorted(staging.rglob('*')):
                        tar.add(path, arcname=f'{name}/{path.relative_to(staging).as_posix()}',
                                recursive=False, filter=normalize)
        if source_status(Path(temp)) or command('git', 'rev-parse', 'HEAD') != commit:
            parser.error('source changed during packaging; commit/stash and retry in a new --output directory')
        # Exclusive publication preserves an existing candidate even if another
        # packaging process raced with the initial existence check.
        os.link(temporary, archive)
        with checksum.open('x') as stream:
            stream.write(f'{digest(archive)}  {archive.name}\n')
    print(archive)


if __name__ == '__main__':
    try:
        main()
    except (OSError, subprocess.CalledProcessError) as error:
        raise SystemExit(f'Packaging failed: {error}')
