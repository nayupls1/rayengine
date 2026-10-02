#!/usr/bin/env python3
"""Verify a built bundle from an empty working directory; requires native OpenGL."""
import argparse
import hashlib
import json
import os
from html.parser import HTMLParser
from pathlib import Path
import subprocess
import tarfile
import tempfile
from urllib.parse import unquote, urlsplit


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


class ReferenceLinks(HTMLParser):
    def __init__(self, page):
        super().__init__()
        self.page = page

    def handle_starttag(self, tag, attrs):
        for name, value in attrs:
            if tag == 'a' and name == 'href' and value:
                link = urlsplit(value)
                # External crate reexports can contain unresolved Rust link
                # disambiguators (e.g. macro@Bundle); check actual page/source URLs.
                if not link.scheme and not link.netloc and link.path.endswith(('.html', '.md', '.py')):
                    assert (self.page.parent / unquote(link.path)).exists(), (self.page, value)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('bundle', type=Path)
    args = parser.parse_args()
    archives = sorted(args.bundle.glob('*.tar.gz')) if args.bundle.is_dir() else [args.bundle]
    if len(archives) != 1:
        parser.error('provide one archive, or a directory containing exactly one .tar.gz')
    archive = archives[0].resolve()
    checksum = archive.with_name(archive.name + '.sha256')
    assert digest(archive) == checksum.read_text().split()[0], 'archive checksum mismatch'
    with tempfile.TemporaryDirectory(prefix='rayengine-package-smoke-') as temp:
        root = Path(temp)
        with tarfile.open(archive) as tar:
            tar.extractall(root, filter='data')
        package, = root.iterdir()
        for line in (package / 'SHA256SUMS').read_text().splitlines():
            expected, relative = line.split('  ', 1)
            assert digest(package / relative) == expected, relative
        manifest = json.loads((package / 'manifest.json').read_text())
        notices = json.loads((package / 'THIRD_PARTY_NOTICES/metadata.json').read_text())
        assert {'raylib', 'raylib-sys', 'glam', 'hecs'} <= {p['name'] for p in notices['packages']}
        native = next(p for p in notices['packages'] if p['name'] == 'raylib-sys')
        for name in ['qoi.h', 'glad.h', 'stb_image.h', 'stb_truetype.h', 'miniaudio.h']:
            path = next(p for p in native['license_files'] if p.endswith('/external/' + name))
            assert (package / path).is_file(), name
        assert (package / 'reference/rayengine/index.html').is_file()
        assert (package / 'reference/rayengine_voxel/index.html').is_file()
        assert (package / 'reference/rayengine_minecraft/index.html').is_file()
        assert (package / 'reference/rayengine_minecraft/guides/release/index.html').is_file()
        assert (package / 'reference/rayengine_minecraft/guides/controls/index.html').is_file()
        for page in (package / 'reference').rglob('*.html'):
            ReferenceLinks(page).feed(page.read_text())
        binary = package / 'minecraft'
        work = root / 'empty-working-directory'
        work.mkdir()
        def run(*args, display=True, success=True):
            env = dict(os.environ)
            if not display:
                for key in ['DISPLAY', 'WAYLAND_DISPLAY']: env.pop(key, None)
            result = subprocess.run([str(binary), *args], cwd=work, env=env,
                                    capture_output=True, text=True, timeout=30)
            assert (result.returncode == 0) == success, (result.returncode, result.stderr)
            return result
        assert '--save PATH' in run('--help', display=False).stdout
        assert run('--version', display=False).stdout.strip() == f"rayengine-minecraft {manifest['version']}"
        assert not list(work.iterdir()), 'help/version created files'
        slot = work / 'saves/world.save'
        image = work / 'scene.png'
        report = work / 'diagnostics.json'
        # Exercise the exact shipped argument parser and default fallback assets.
        run('--seed', '123', '--save', str(slot), '--hidden', '--frames', '60', '--uncapped',
            '--screenshot', str(image), '--diagnostics', str(report), '--workload', 'minecraft_package_v1')
        data = slot.read_bytes()
        assert data[:8] == b'RAYSAVE\0'
        assert json.loads(data[28:])['generator']['seed'] == 123
        assert image.read_bytes().startswith(b'\x89PNG\r\n\x1a\n')
        json.loads(report.read_text())
        run('--save', str(slot), '--hidden', '--frames', '2', '--uncapped')
        assert json.loads(slot.read_bytes()[28:])['generator']['seed'] == 123
        previous = slot.read_bytes()
        conflict = run('--seed', '124', '--save', str(slot), '--hidden', '--frames', '1', success=False)
        assert 'requested seed differs' in conflict.stderr
        assert slot.read_bytes() == previous
        run('--save', str(slot), '--atomic-save', '--hidden', '--frames', '2', '--uncapped')
    print(f"Bundle checks passed: checksums, offline docs, help/version, fallback launch, save/reload ({manifest['target']}, {manifest['backend']}).")


if __name__ == '__main__':
    main()
