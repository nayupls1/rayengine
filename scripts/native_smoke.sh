#!/usr/bin/env bash
set -euo pipefail
rayengine_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$rayengine_root"
features=()
demo_features=()
backend=${RAYENGINE_BACKEND:-x11}
if [[ "$backend" == wayland ]]; then
    features=(--features wayland)
    demo_features=(--features rayengine/wayland)
elif [[ "$backend" != x11 ]]; then
    echo 'RAYENGINE_BACKEND must be x11 or wayland' >&2
    exit 2
fi
cargo test -p rayengine "${features[@]}" native_render_smoke -- --ignored --test-threads=1
cargo build -p rayengine-demos "${demo_features[@]}" --bins
rayengine_target=${CARGO_TARGET_DIR:-"$rayengine_root/target"}
for game in arena meadow; do
    "$rayengine_target/debug/$game" --hidden --frames 30 --size 1280x720 --screenshot "artifacts/smoke/$backend/$game-wide.png"
    "$rayengine_target/debug/$game" --hidden --frames 30 --size 800x1000 --screenshot "artifacts/smoke/$backend/$game-portrait.png"
done
echo "Native smoke screenshots: artifacts/smoke/$backend"
