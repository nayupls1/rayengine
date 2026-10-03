#!/usr/bin/env python3
"""Install the packaged CLI and compile fresh consumers outside the checkout."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile

from release_check import PACKAGES, metadata


def run(command, **kwargs):
    print("+ " + " ".join(map(str, command)), flush=True)
    return subprocess.run(command, check=True, **kwargs)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--registry", action="store_true", help="Use actual crates.io packages after publishing")
    parser.add_argument("--toolchain", help="Compile fresh consumers with this Rust toolchain")
    args = parser.parse_args()
    data = metadata()
    version = next(p["version"] for p in data["packages"] if p["name"] == "rayengine")
    environment = os.environ.copy()
    if args.toolchain:
        environment["RUSTUP_TOOLCHAIN"] = args.toolchain
    environment["CARGO_TARGET_DIR"] = str(Path(data["target_directory"]) / "release-smoke")
    with tempfile.TemporaryDirectory(prefix="rayengine-release-") as temporary:
        directory = Path(temporary)
        install_root = directory / "cli"
        patches = ""
        install = ["cargo", "install", "--locked", "--debug", "--root", str(install_root)]
        if args.registry:
            install += ["--registry", "crates-io", "--version", version, "rayengine-cli"]
        else:
            for name in PACKAGES:
                archive_path = Path(data["target_directory"]) / "package" / f"{name}-{version}.crate"
                with tarfile.open(archive_path, "r:gz") as archive:
                    archive.extractall(directory / "packages", filter="data")
            install += ["--path", str(directory / "packages" / f"rayengine-cli-{version}")]
            core_candidate = directory / "packages" / f"rayengine-core-{version}"
            install += ["--config", f"patch.crates-io.rayengine-core.path={json.dumps(str(core_candidate))}"]
            patches = "\n[patch.crates-io]\n" + "".join(
                f"{name} = {{ path = {json.dumps(str(directory / 'packages' / f'{name}-{version}'))} }}\n"
                for name in PACKAGES if name not in {"rayengine-cli", "rayengine-voxel"}
            )
        run(install, cwd=directory, env=environment)
        binary = install_root / "bin/rayengine"
        run([str(binary), "--version"], cwd=directory, env=environment)
        for kind in ("2d", "3d", "plugin"):
            project = directory / f"starter-{kind}"
            command = [str(binary), "new-plugin" if kind == "plugin" else "new", str(project)]
            if kind != "plugin":
                command += ["--kind", kind]
            run(command, cwd=directory, env=environment)
            manifest = project / "Cargo.toml"
            original = manifest.read_text()
            if f'rayengine = "{version}"' not in original or "path =" in original:
                raise ValueError("Default scaffolding must depend on the published SDK version")
            manifest.write_text(original + patches)
            run(["cargo", "check", "--manifest-path", str(manifest)], cwd=directory, env=environment)
            if kind == "3d":
                run(["cargo", "check", "--manifest-path", str(manifest), "--features", "rayengine/wayland"], cwd=directory, env=environment)
        voxel_patches = patches
        if not args.registry:
            voxel_patches += f'rayengine-voxel = {{ path = {json.dumps(str(directory / "packages" / f"rayengine-voxel-{version}"))} }}\n'
        voxel = directory / "voxel-consumer"
        (voxel / "src").mkdir(parents=True)
        (voxel / "Cargo.toml").write_text(
            f'[package]\nname = "voxel-consumer"\nversion = "0.1.0"\nedition = "2024"\n'
            f'[dependencies]\nrayengine-voxel = "{version}"\n[workspace]\n' + voxel_patches
        )
        (voxel / "src/main.rs").write_text(
            "fn main() { let _ = rayengine_voxel::BlockRegistry::new(); }\n"
        )
        run(["cargo", "check"], cwd=voxel, env=environment)
        run(["cargo", "check", "--features", "rayengine-voxel/render"], cwd=voxel, env=environment)
        print("Packaged CLI, 2D/3D/plugin starters, Wayland and CPU/render voxel consumers passed", flush=True)


if __name__ == "__main__":
    main()
