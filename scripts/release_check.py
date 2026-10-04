#!/usr/bin/env python3
"""Validate coordinated release metadata and the actual Cargo archives."""

import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tarfile
import tomllib

ROOT = Path(__file__).resolve().parent.parent
PACKAGES = ("rayengine-core", "rayengine", "rayengine-voxel", "rayengine-cli")


def metadata():
    return json.loads(subprocess.check_output(
        ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"],
        cwd=ROOT, text=True,
    ))


def validate(data, requested_version=None):
    members = {p["name"]: p for p in data["packages"] if p["id"] in data["workspace_members"]}
    published = {name for name, package in members.items() if package["publish"] != []}
    if published != set(PACKAGES):
        raise ValueError(f"Expected publishable crates {PACKAGES}, found {sorted(published)}")
    version = members["rayengine"]["version"]
    if not re.fullmatch(r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)", version):
        raise ValueError("Release version must be a stable major.minor.patch version")
    if requested_version is not None and requested_version != version:
        raise ValueError(f"Requested {requested_version!r}, but the committed version is {version}")
    for name in PACKAGES:
        package = members[name]
        if package["version"] != version:
            raise ValueError(f"{name} must share release version {version}")
        for field in ("description", "license", "repository", "readme", "rust_version", "keywords", "categories"):
            if not package.get(field):
                raise ValueError(f"{name} is missing {field}")
        folder = Path(package["manifest_path"]).parent
        if (folder / "LICENSE").read_bytes() != (ROOT / "LICENSE").read_bytes():
            raise ValueError(f"{name} must include the current root MIT license text")
        for dependency in package["dependencies"]:
            if dependency.get("path") and dependency["kind"] != "dev":
                if dependency["name"] not in published or dependency["req"] != f"^{version}":
                    raise ValueError(f"{name}: internal dependency {dependency['name']} must use version {version}")
    return version


def inspect_archives(version, directory):
    checksums = []
    for name in PACKAGES:
        path = directory / f"{name}-{version}.crate"
        with tarfile.open(path, "r:gz") as archive:
            prefix = f"{name}-{version}/"
            files = {member.name.removeprefix(prefix) for member in archive.getmembers() if member.isfile()}
            required = {"Cargo.toml", "Cargo.lock", "README.md", "LICENSE", ".cargo_vcs_info.json"}
            if name == "rayengine-cli":
                required |= {"src/templates/2d.rs", "src/templates/3d.rs", "src/templates/plugin.rs", "src/templates/rayengine.toml",
                             "src/templates/topdown.rs", "src/templates/topdown.toml",
                             "src/templates/platformer.rs", "src/templates/platformer.toml",
                             "src/lifecycle.rs", "src/package.rs", "src/plugins.rs", "src/watch.rs"}
            if name == "rayengine":
                required |= {"docs/quickstart.md", "docs/project_manifest.md", "src/assets/materials/default.fs"}
            if name == "rayengine-core":
                required.add("src/manifest.rs")
            if name == "rayengine-voxel":
                required.add("src/render/repeat.fs")
            if not required <= files:
                raise ValueError(f"{name}: archive missing {sorted(required - files)}")
            for file in files:
                parts = Path(file).parts
                if any(part in {"target", "artifacts", "local-assets", "local-saves"} or part.startswith(".env") for part in parts):
                    raise ValueError(f"{name}: unexpected package file {file}")
            license_text = archive.extractfile(prefix + "LICENSE").read()
            if license_text != (ROOT / "LICENSE").read_bytes():
                raise ValueError(f"{name}: incorrect packaged license")
            manifest = tomllib.loads(archive.extractfile(prefix + "Cargo.toml").read().decode())
            if manifest["package"]["name"] != name or manifest["package"]["version"] != version:
                raise ValueError(f"{name}: incorrect packaged identity")
            for section in ("dependencies", "build-dependencies"):
                if any("path" in dep or "git" in dep for dep in manifest.get(section, {}).values()):
                    raise ValueError(f"{name}: non-registry dependency in normalized manifest")
        checksums.append(f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}\n")
        print(f"Validated {path.name}: {len(files)} files, {path.stat().st_size} bytes", flush=True)
    (directory / "SHA256SUMS").write_text("".join(checksums))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", help="Require this exact committed release version")
    parser.add_argument("--package", action="store_true", help="Package and verify all crates together, then inspect their archives")
    parser.add_argument("--allow-dirty", action="store_true", help="Local preparation only; never used in publishing CI")
    args = parser.parse_args()
    data = metadata()
    version = validate(data, args.version)
    print(f"Release {version}: {', '.join(PACKAGES)}", flush=True)
    if args.package:
        command = ["cargo", "package", "--locked", "--registry", "crates-io"]
        if args.allow_dirty:
            command.append("--allow-dirty")
        for package in PACKAGES:
            command += ["-p", package]
        subprocess.run(command, cwd=ROOT, check=True)
        inspect_archives(version, Path(data["target_directory"]) / "package")


if __name__ == "__main__":
    main()
