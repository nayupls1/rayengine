#!/usr/bin/env bash
set -euo pipefail
rayengine_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$rayengine_root"
mode=${1:-}
baseline=${2:-}
if [[ "$mode" != save && "$mode" != compare ]] || [[ ! "$baseline" =~ ^[a-zA-Z0-9][a-zA-Z0-9._-]*$ ]]; then
    echo 'Usage: scripts/render_benchmark.sh {save|compare} BASELINE [criterion arguments...]' >&2
    exit 2
fi
shift 2
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-"$rayengine_root/target"}
export CRITERION_HOME="$CARGO_TARGET_DIR/criterion"
export RAYENGINE_RENDER_BENCH=1
backend=${RAYENGINE_BACKEND:-x11}
features=()
if [[ "$backend" == wayland ]]; then
    features=(--features wayland)
elif [[ "$backend" != x11 ]]; then
    echo 'RAYENGINE_BACKEND must be x11 or wayland' >&2
    exit 2
fi
flag=--save-baseline
if [[ "$mode" == compare ]]; then flag=--baseline; fi
run_dir="artifacts/benchmarks/$baseline/render-$mode-$(date -u +%Y%m%dT%H%M%SZ)"
mkdir -p "$run_dir"
python3 scripts/benchmark_metadata.py before "$run_dir" "$mode" "$baseline" "$@"
python3 - "$run_dir" "$backend" <<'PY'
import json, os, shutil, subprocess, sys
from pathlib import Path
path = Path(sys.argv[1]) / 'metadata.json'
data = json.loads(path.read_text())
data.update(measurement='Native OpenGL draw submission/upload wall time; includes driver stalls; no GPU timer', backend=sys.argv[2], render_target=[64,64], vsync=False, renderer_note=os.environ.get('RAYENGINE_RENDERER_INFO',''))
data['gl_environment'] = {key: os.environ.get(key) for key in
                          ('LIBGL_ALWAYS_SOFTWARE', '__GLX_VENDOR_LIBRARY_NAME', 'MESA_LOADER_DRIVER_OVERRIDE')}
if sys.argv[2] == 'x11' and shutil.which('glxinfo'):
    result = subprocess.run(['glxinfo','-B'],capture_output=True,text=True,timeout=10)
    data['glxinfo'] = result.stdout
    data['glxinfo_status'] = result.returncode
elif sys.argv[2] == 'x11' and shutil.which('glewinfo'):
    result = subprocess.run(['glewinfo'],capture_output=True,text=True,timeout=10)
    data['glewinfo_header'] = '\n'.join(result.stdout.splitlines()[:10])
    data['glewinfo_status'] = result.returncode
path.write_text(json.dumps(data,indent=2)+'\n')
PY
cargo bench --locked -p rayengine "${features[@]}" --bench draw_submission -- "$flag" "$baseline" "$@"
voxel_features=(--features render)
if [[ "$backend" == wayland ]]; then voxel_features=(--features render,rayengine/wayland); fi
cargo bench --locked -p rayengine-voxel "${voxel_features[@]}" --bench voxel_render -- "$flag" "$baseline" "$@"
minecraft_features=(--features render)
if [[ "$backend" == wayland ]]; then minecraft_features=(--features render,rayengine/wayland); fi
cargo bench --locked -p rayengine-minecraft "${minecraft_features[@]}" --bench textures_render -- "$flag" "$baseline" "$@"
cargo bench --locked -p rayengine-minecraft "${minecraft_features[@]}" --bench survival_render -- "$flag" "$baseline" "$@"
python3 scripts/benchmark_metadata.py after "$run_dir" "$mode" "$baseline"
echo "Native benchmark snapshot: $rayengine_root/$run_dir"
