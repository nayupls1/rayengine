#!/usr/bin/env bash
# Exercise native audio with a clocked silent sink, without hardware speakers.
set -euo pipefail
rayengine_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$rayengine_root"
if ! command -v pulseaudio >/dev/null; then
    echo 'native_audio_smoke.sh requires pulseaudio (installed by the Linux CI setup).' >&2
    exit 1
fi
audio_directory=$(mktemp -d)
audio_pid=
cleanup() {
    if [[ -n "$audio_pid" ]]; then
        kill "$audio_pid" 2>/dev/null || true
        wait "$audio_pid" 2>/dev/null || true
    fi
    rm -rf -- "$audio_directory"
}
trap cleanup EXIT
# A private socket and no default modules keep this isolated from desktop audio.
# PulseAudio's null sink advances at real sample rate; ALSA's null PCM does not.
XDG_RUNTIME_DIR="$audio_directory" XDG_CONFIG_HOME="$audio_directory/config" \
    XDG_CACHE_HOME="$audio_directory/cache" pulseaudio --daemonize=no --exit-idle-time=-1 \
    --use-pid-file=no -n \
    --load="module-native-protocol-unix socket=$audio_directory/pulse.sock auth-anonymous=1 auth-cookie-enabled=0" \
    --load="module-null-sink sink_name=rayengine_test" \
    >"$audio_directory/pulse.log" 2>&1 &
audio_pid=$!
for _ in {1..100}; do
    if [[ -S "$audio_directory/pulse.sock" ]]; then break; fi
    if ! kill -0 "$audio_pid" 2>/dev/null; then
        cat "$audio_directory/pulse.log" >&2
        exit 1
    fi
    sleep 0.05
done
if [[ ! -S "$audio_directory/pulse.sock" ]]; then
    cat "$audio_directory/pulse.log" >&2
    echo 'Timed out waiting for the private audio server.' >&2
    exit 1
fi
export PULSE_SERVER="unix:$audio_directory/pulse.sock"
export RAYENGINE_AUDIO_REALTIME=1
cargo test --locked -p rayengine native_audio -- --ignored --test-threads=1
cargo run --locked -p rayengine --example audio -- --hidden --frames 120 --screenshot artifacts/smoke/audio.png

# Embervault loads original streamed tracks as part of its native render probe.
cargo test --locked -p rayengine-demos native_dungeon -- --ignored --test-threads=1
RAYENGINE_DUNGEON_SAVE="$audio_directory/dungeon.save" cargo run --locked -p rayengine-demos --bin dungeon -- --hidden --frames 30 --screenshot artifacts/smoke/dungeon-title.png
python3 scripts/dungeon_package_smoke.py
