#!/usr/bin/env python3
"""Build a real tilemap starter, then run its extracted bundle without its sources.

Use --archive in a runtime-only Linux container to skip all Cargo/SDK build work.
The launcher receives no Cargo/Rust environment and runs from an unrelated cwd.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parent.parent


def validate(archive):
    with tempfile.TemporaryDirectory(prefix="rayengine-bundle-runtime-") as temporary:
        scratch = Path(temporary)
        extracted = scratch / "extracted"
        extracted.mkdir()
        with tarfile.open(archive) as bundle:
            bundle.extractall(extracted, filter="data")
        folders = list(extracted.iterdir())
        assert len(folders) == 1 and folders[0].is_dir()
        folder = folders[0]
        notices_path = folder / "THIRD_PARTY_NOTICES/metadata.json"
        assert notices_path.is_file()
        notices = json.loads(notices_path.read_text())["packages"]
        tilemap = next(package for package in notices if package["name"] == "rayengine-tilemap")
        assert tilemap["files"], "external workspace plugins must retain their shared license"
        assert (folder / "README.txt").is_file()
        assert (folder / "runtime-libraries.txt").is_file()
        manifest = tomllib.loads((folder / "rayengine.toml").read_text())
        assert (folder / manifest["assets"]["roots"][0] / "level.toml").is_file()
        libraries = subprocess.run(["ldd", str(folder / "bin/game")], text=True, capture_output=True, check=True).stdout
        assert "not found" not in libraries, libraries
        # Keep display/driver settings but remove all build and manifest overrides.
        env = {k: v for k, v in os.environ.items() if not k.startswith(("CARGO", "RUST", "RAYENGINE"))}
        env["PATH"] = "/usr/bin:/bin"
        env["LIBGL_ALWAYS_SOFTWARE"] = "1"
        screenshot = scratch / "frame.png"
        result = subprocess.run([str(folder / "launch"), "--frames", "3", "--hidden", "--uncapped", "--screenshot", str(screenshot)],
                                cwd=scratch, env=env, text=True, capture_output=True, timeout=45)
        if result.returncode:
            raise SystemExit(result.stdout + result.stderr)
        assert screenshot.read_bytes().startswith(b"\x89PNG\r\n\x1a\n")
        print(json.dumps(dict(schema_version=1, ok=True, archive=str(archive), runtime_libraries=libraries,
                              screenshot_bytes=screenshot.stat().st_size)))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", type=Path, help="validate an existing tarball; no build tools required")
    parser.add_argument("--output", type=Path, default=ROOT / "artifacts/cli-package-smoke")
    args = parser.parse_args()
    if args.archive:
        validate(args.archive.resolve())
        return
    env = os.environ.copy()
    target = Path(env.get("CARGO_TARGET_DIR", ROOT / "target")).resolve()
    env["CARGO_TARGET_DIR"] = str(target)
    subprocess.run(["cargo", "build", "--locked", "-p", "rayengine-cli"], cwd=ROOT, env=env, check=True)
    binary = target / "debug/rayengine"
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="rayengine-bundle-source-") as temporary:
        game = Path(temporary) / "bundle-smoke"
        for command in [
            ["new", str(game), "--template", "topdown", "--sdk-path", str(ROOT / "crates/rayengine")],
            ["package", str(game), "--profile", "dev", "--output", str(output)],
        ]:
            result = subprocess.run([str(binary), "--json", *command], env=env, capture_output=True, text=True, check=True)
            response = json.loads(result.stdout)
            assert response["ok"], response
        archive = Path(response["data"]["archive"])
        # The compile-time manifest path now points to a nonexistent directory.
        shutil.rmtree(game)
        validate(archive)


if __name__ == "__main__":
    main()
