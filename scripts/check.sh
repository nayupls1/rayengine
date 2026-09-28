#!/usr/bin/env bash
set -euo pipefail
rayengine_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$rayengine_root"
cargo fmt --all --check
rustfmt --edition 2024 --check crates/rayengine-cli/src/templates/*.rs
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps
