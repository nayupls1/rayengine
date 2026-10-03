#!/usr/bin/env bash
# Exercise native audio even on CI workers without a hardware output device.
set -euo pipefail
rayengine_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$rayengine_root"
audio_config=$(mktemp)
trap 'rm -f "$audio_config"' EXIT
cat > "$audio_config" <<'ALSA'
pcm.!default {
    type null
    hint.description "rayengine silent test output"
}
ALSA
export ALSA_CONFIG_PATH="$audio_config"
# Force miniaudio to try ALSA rather than an existing PulseAudio server.
export PULSE_SERVER=unix:/nonexistent-rayengine-audio-test-server
cargo test --locked -p rayengine native_audio -- --ignored --test-threads=1
cargo run --locked -p rayengine --example audio -- --hidden --frames 120 --screenshot artifacts/smoke/audio.png
