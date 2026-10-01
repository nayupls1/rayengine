#!/usr/bin/env bash
set -euo pipefail
rayengine_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$rayengine_root"
cargo fmt --all --check
rustfmt --edition 2024 --check crates/rayengine-cli/src/templates/*.rs
cargo clippy --workspace --all-targets --features rayengine-voxel/render,rayengine-minecraft/render -- -D warnings
cargo test --workspace --features rayengine-voxel/render,rayengine-minecraft/render
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --features rayengine-voxel/render,rayengine-minecraft/render
