#!/usr/bin/env python3
"""Capture benchmark provenance and portable estimate summaries using only stdlib."""

import datetime
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys


def command(*args):
    return subprocess.check_output(args, text=True).strip()


def main():
    stage, destination, mode, baseline, *extra = sys.argv[1:]
    destination = Path(destination)
    if stage == "before":
        cpu_info = Path("/proc/cpuinfo")
        cpu = next((line.split(":", 1)[1].strip() for line in cpu_info.read_text().splitlines()
                    if line.startswith("model name")), platform.processor()) if cpu_info.exists() else platform.processor()
        data = {
            "schema_version": 1,
            "utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
            "commit": command("git", "rev-parse", "HEAD"),
            "dirty_files": command("git", "status", "--short").splitlines(),
            "rustc": command("rustc", "-Vv"),
            "cargo": command("cargo", "--version"),
            "platform": platform.platform(),
            "cpu": cpu,
            "logical_cpus": os.cpu_count(),
            "rustflags": os.environ.get("RUSTFLAGS", ""),
            "measurement": "CPU-only; no window or GPU initialized",
            "mode": mode,
            "baseline": baseline,
            "criterion_arguments": extra,
        }
        if os.environ.get("RAYENGINE_SAVE_IO_BENCH") == "1":
            base = Path(os.environ.get("RAYENGINE_SAVE_BENCH_DIR",
                                       "artifacts/benchmarks/save-io-fixtures")).absolute()
            existing = base
            while not existing.exists() and existing != existing.parent:
                existing = existing.parent
            filesystem = None
            if sys.platform.startswith("linux") and shutil.which("stat"):
                try:
                    filesystem = command("stat", "-f", "-c", "%T", str(existing))
                except subprocess.CalledProcessError:
                    pass
            data["measurement"] = "CPU and optional filesystem I/O; no window or GPU initialized"
            data["save_io"] = {
                "directory": str(base),
                "filesystem": filesystem,
                "payload_bytes": [65536, 1048576],
                "sample_size": 10,
                "warm_up_seconds": 0.25,
                "measurement_seconds": 1,
                "durable_enabled": sys.platform.startswith("linux"),
                "load_cache": "warm OS page cache; no cache eviction",
            }
        (destination / "metadata.json").write_text(json.dumps(data, indent=2) + "\n")
        return

    criterion = Path(os.environ["CRITERION_HOME"])
    source_name = baseline if mode == "save" else "new"
    started = (destination / "metadata.json").stat().st_mtime_ns
    estimates = {}
    for path in sorted(criterion.rglob("estimates.json")):
        if path.parent.name != source_name or path.stat().st_mtime_ns < started:
            continue
        relative = path.relative_to(criterion)
        name = "/".join(relative.parts[:-2])
        data = json.loads(path.read_text())
        estimates[name] = {
            "mean_ns": data["mean"]["point_estimate"],
            "median_ns": data["median"]["point_estimate"],
            "mean_confidence_interval": data["mean"]["confidence_interval"],
        }
        copy_to = destination / "criterion" / relative.parent.parent
        shutil.copytree(path.parent, copy_to, dirs_exist_ok=True)
    if not estimates:
        raise SystemExit("No estimates exported; check benchmark arguments and CRITERION_HOME")
    (destination / "results.json").write_text(json.dumps({"schema_version": 1, "benchmarks": estimates}, indent=2) + "\n")


if __name__ == "__main__":
    main()
