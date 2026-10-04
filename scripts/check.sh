#!/usr/bin/env bash
set -euo pipefail
rayengine_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$rayengine_root"
python3 scripts/test_import_minecraft_textures.py
python3 scripts/test_package_minecraft_source.py
cargo fmt --all --check
rustfmt --edition 2024 --check crates/rayengine-cli/src/templates/*.rs
cargo clippy --locked --workspace --all-targets --features rayengine-voxel/render,rayengine-minecraft/render,rayengine-particles/render,rayengine-tilemap/render -- -D warnings
cargo test --locked --workspace --features rayengine-voxel/render,rayengine-minecraft/render,rayengine-particles/render,rayengine-tilemap/render
cargo test --locked -p rayengine --example physics
RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --no-deps --features rayengine-voxel/render,rayengine-minecraft/render,rayengine-particles/render,rayengine-tilemap/render
