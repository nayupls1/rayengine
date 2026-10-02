#!/usr/bin/env bash
set -euo pipefail
rayengine_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$rayengine_root"
mode=${1:-}
baseline=${2:-}
if [[ "$mode" != save && "$mode" != compare ]] || [[ ! "$baseline" =~ ^[a-zA-Z0-9][a-zA-Z0-9._-]*$ ]]; then
    echo 'Usage: scripts/benchmark.sh {save|compare} BASELINE [criterion arguments...]' >&2
    exit 2
fi
shift 2
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-"$rayengine_root/target"}
export CRITERION_HOME="$CARGO_TARGET_DIR/criterion"
if [[ "$mode" == compare ]]; then
    flag=--baseline
    if [[ ! -d "$CRITERION_HOME" ]]; then
        echo 'No Criterion baselines exist. Run save first.' >&2
        exit 1
    fi
else
    flag=--save-baseline
fi
run_dir="artifacts/benchmarks/$baseline/$mode-$(date -u +%Y%m%dT%H%M%SZ)"
mkdir -p "$run_dir"
python3 scripts/benchmark_metadata.py before "$run_dir" "$mode" "$baseline" "$@"
cargo bench -p rayengine-core --bench primitives -- "$flag" "$baseline" "$@"
cargo bench -p rayengine-demos --bench gameplay -- "$flag" "$baseline" "$@"
cargo bench -p rayengine-voxel --bench voxel -- "$flag" "$baseline" "$@"
cargo bench -p rayengine-minecraft --bench generation -- "$flag" "$baseline" "$@"
cargo bench -p rayengine-minecraft --bench textures -- "$flag" "$baseline" "$@"
cargo bench -p rayengine-minecraft --bench survival -- "$flag" "$baseline" "$@"
cargo bench -p rayengine-minecraft --bench persistence -- "$flag" "$baseline" "$@"
cargo bench -p rayengine-particles --bench particles -- "$flag" "$baseline" "$@"
python3 scripts/benchmark_metadata.py after "$run_dir" "$mode" "$baseline"
echo "Benchmark snapshot: $rayengine_root/$run_dir"
