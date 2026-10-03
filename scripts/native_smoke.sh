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
cargo test -p rayengine "${features[@]}" native_quality -- --ignored --test-threads=1
cargo test -p rayengine "${features[@]}" native_render_smoke -- --ignored --test-threads=1
cargo test -p rayengine "${features[@]}" native_mesh -- --ignored --test-threads=1
cargo test -p rayengine "${features[@]}" native_material -- --ignored --test-threads=1
cargo test -p rayengine "${features[@]}" native_lighting -- --ignored --test-threads=1
cargo test -p rayengine "${features[@]}" native_upload -- --ignored --test-threads=1
cargo test -p rayengine "${features[@]}" native_state -- --ignored --test-threads=1
cargo test -p rayengine "${features[@]}" native_ui -- --ignored --test-threads=1
RAYENGINE_FONT_ARTIFACTS="$rayengine_root/artifacts/smoke/fonts" cargo test -p rayengine "${features[@]}" native_font -- --ignored --test-threads=1
cargo test -p rayengine "${features[@]}" native_sprite -- --ignored --test-threads=1
cargo test -p rayengine "${features[@]}" native_diagnostics -- --ignored --test-threads=1
cargo test -p rayengine-demos "${demo_features[@]}" native_gameplay -- --ignored --test-threads=1
cargo test -p rayengine-beacons "${demo_features[@]}" native_plugin -- --ignored --test-threads=1
cargo test -p rayengine-voxel --features render "${demo_features[@]}" native_voxel -- --ignored --test-threads=1
cargo test -p rayengine-minecraft --features render "${demo_features[@]}" native_minecraft -- --ignored --test-threads=1
cargo test -p rayengine-particles --features render "${demo_features[@]}" native_particles -- --ignored --test-threads=1
cargo run -p rayengine-particles --features render "${demo_features[@]}" --example effects -- --hidden --frames 120 --size 1280x720 --screenshot "artifacts/smoke/$backend/particles-wide.png"
cargo run -p rayengine-particles --features render "${demo_features[@]}" --example effects -- --hidden --frames 120 --size 800x1000 --screenshot "artifacts/smoke/$backend/particles-portrait.png"
cargo build -p rayengine-demos "${demo_features[@]}" --bins
rayengine_target=${CARGO_TARGET_DIR:-"$rayengine_root/target"}
for game in arena meadow; do
    "$rayengine_target/debug/$game" --hidden --frames 30 --size 1280x720 --screenshot "artifacts/smoke/$backend/$game-wide.png"
    "$rayengine_target/debug/$game" --hidden --frames 30 --size 800x1000 --screenshot "artifacts/smoke/$backend/$game-portrait.png"
done
cargo run -p rayengine "${features[@]}" --example states -- --hidden --frames 30 --size 1280x720 --screenshot "artifacts/smoke/$backend/states-wide.png"
cargo run -p rayengine "${features[@]}" --example menu -- --hidden --frames 30 --size 1280x720 --screenshot "artifacts/smoke/$backend/menu-wide.png"
cargo run -p rayengine "${features[@]}" --example menu -- --hidden --frames 30 --size 800x1000 --screenshot "artifacts/smoke/$backend/menu-portrait.png"
cargo run -p rayengine "${features[@]}" --example controls -- --hidden --frames 30 --size 1280x720 --screenshot "artifacts/smoke/$backend/controls-wide.png"
cargo run -p rayengine "${features[@]}" --example controls -- --hidden --frames 30 --size 800x1000 --screenshot "artifacts/smoke/$backend/controls-portrait.png"
cargo run -p rayengine "${features[@]}" --example sprites -- --hidden --frames 30 --size 1280x720 --screenshot "artifacts/smoke/$backend/sprites-wide.png"
cargo run -p rayengine "${features[@]}" --example sprites -- --hidden --frames 30 --size 800x1000 --screenshot "artifacts/smoke/$backend/sprites-portrait.png"
cargo run -p rayengine "${features[@]}" --example pathfinding -- --hidden --frames 60 --size 1280x720 --screenshot "artifacts/smoke/$backend/pathfinding-wide.png"
cargo run -p rayengine "${features[@]}" --example pathfinding -- --hidden --frames 60 --size 800x1000 --screenshot "artifacts/smoke/$backend/pathfinding-portrait.png"
cargo run -p rayengine "${features[@]}" --example first_person -- --hidden --frames 30 --size 1280x720 --screenshot "artifacts/smoke/$backend/first-person-wide.png"
cargo run -p rayengine "${features[@]}" --example first_person -- --hidden --frames 30 --size 800x1000 --screenshot "artifacts/smoke/$backend/first-person-portrait.png"
cargo run -p rayengine-beacons "${demo_features[@]}" --example composition -- --hidden --frames 30 --size 1280x720 --screenshot "artifacts/smoke/$backend/plugins-wide.png"
cargo run -p rayengine-beacons "${demo_features[@]}" --example composition -- --hidden --frames 30 --size 800x1000 --screenshot "artifacts/smoke/$backend/plugins-portrait.png"
cargo run -p rayengine-voxel --features render "${demo_features[@]}" --example render -- --hidden --frames 30 --size 1280x720 --screenshot "artifacts/smoke/$backend/voxel-wide.png"
cargo run -p rayengine-voxel --features render "${demo_features[@]}" --example render -- --hidden --frames 30 --size 800x1000 --screenshot "artifacts/smoke/$backend/voxel-portrait.png"
cargo run -p rayengine-voxel --features render "${demo_features[@]}" --example stream_render -- --hidden --frames 120 --size 1280x720 --screenshot "artifacts/smoke/$backend/voxel-stream-wide.png"
cargo run -p rayengine-voxel --features render "${demo_features[@]}" --example stream_render -- --hidden --frames 120 --size 800x1000 --screenshot "artifacts/smoke/$backend/voxel-stream-portrait.png"
cargo run -p rayengine-minecraft --features render "${demo_features[@]}" --bin minecraft -- --seed 42 --hidden --frames 240 --size 1280x720 --screenshot "artifacts/smoke/$backend/minecraft-terrain-wide.png"
cargo run -p rayengine-minecraft --features render "${demo_features[@]}" --bin minecraft -- --seed 42 --hidden --frames 240 --size 800x1000 --screenshot "artifacts/smoke/$backend/minecraft-terrain-portrait.png"
if [[ -n "${RAYENGINE_MINECRAFT_TEXTURES:-}" ]]; then
    for size in 1280x720 800x1000; do
        cargo run -p rayengine-minecraft --features render "${demo_features[@]}" --bin minecraft -- --textures "$RAYENGINE_MINECRAFT_TEXTURES" --seed 42 --hidden --frames 240 --size "$size" --screenshot "artifacts/smoke/$backend/minecraft-import-$size.png"
    done
fi
cargo run -p rayengine "${features[@]}" --example lighting -- --hidden --frames 30 --screenshot "artifacts/smoke/$backend/lighting.png"
echo "Native smoke screenshots: artifacts/smoke/$backend"
