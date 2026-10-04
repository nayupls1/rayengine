#!/usr/bin/env python3
"""Compare direct rendering with an explicitly empty post-processing chain."""
import argparse
import datetime
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess


def command(*args):
    return subprocess.check_output(args, text=True).strip()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, default=Path('artifacts/post-processing'))
    parser.add_argument('--frames', type=int, default=400)
    parser.add_argument('--repeats', type=int, default=3)
    parser.add_argument('--size', default='1280x720')
    args = parser.parse_args()
    if args.frames <= 0 or args.repeats <= 0:
        parser.error('frames and repeats must be positive')
    root = Path(__file__).resolve().parents[1]
    os.chdir(root)
    args.output.mkdir(parents=True, exist_ok=True)
    backend = os.environ.get('RAYENGINE_BACKEND', 'x11')
    if backend not in ('x11', 'wayland'):
        parser.error('RAYENGINE_BACKEND must be x11 or wayland')
    build = ['cargo', 'build', '--locked', '--release', '-p', 'rayengine', '--example', 'post_processing']
    if backend == 'wayland':
        build += ['--features', 'wayland']
    subprocess.run(build, check=True)
    binary = Path(os.environ.get('CARGO_TARGET_DIR', root / 'target')) / 'release/examples/post_processing'
    profiles = ['direct', 'empty']
    records = {profile: [] for profile in profiles}
    reference_draws = None
    for repeat in range(args.repeats):
        # Rotate run order to reduce systematic warmup/thermal bias.
        for profile in profiles[repeat % len(profiles):] + profiles[:repeat % len(profiles)]:
            report = args.output / f'{profile}-{repeat}.json'
            run = [str(binary), profile, '--hidden', '--uncapped', '--frames', str(args.frames),
                   '--size', args.size, '--diagnostics', str(report), '--workload', 'post-processing/mixed.v1']
            if repeat == 0:
                run += ['--screenshot', str(args.output / f'{profile}.png')]
            subprocess.run(run, check=True)
            data = json.loads(report.read_text())
            if reference_draws is None:
                reference_draws = data['draws']
            if data['draws'] != reference_draws:
                raise SystemExit('Post-processing modes submitted different workloads')
            records[profile].append(data)
    cpu = platform.processor()
    if Path('/proc/cpuinfo').exists():
        cpu = next((line.split(':', 1)[1].strip() for line in Path('/proc/cpuinfo').read_text().splitlines()
                    if line.startswith('model name')), cpu)
    summary = {
        'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
        'commit': command('git', 'rev-parse', 'HEAD'),
        'dirty_files': command('git', 'status', '--short').splitlines(),
        'rustc': command('rustc', '-Vv'), 'cargo': command('cargo', '--version'),
        'platform': platform.platform(), 'cpu': cpu, 'backend': backend,
        'environment': {key: os.environ.get(key, '') for key in
                        ['DISPLAY', 'WAYLAND_DISPLAY', 'LIBGL_ALWAYS_SOFTWARE', 'RUSTFLAGS']},
        'frames_per_run': args.frames, 'repeats': args.repeats,
        'measurement': 'Uncapped frame wall time including initial target allocation, resolve, driver stalls and presentation; no GPU timer',
        'workload': 'post-processing/mixed.v1', 'draws': reference_draws, 'profiles': {},
    }
    for profile, runs in records.items():
        settings = runs[0]['settings']
        if any(run['settings'] != settings for run in runs):
            raise SystemExit(f'Environment/settings changed between {profile} runs')
        timings = [run['frame']['total_ns'] / run['frames'] / 1e6 for run in runs]
        summary['profiles'][profile] = {
            'settings': settings, 'frame_ms_per_run': timings,
            'median_frame_ms': statistics.median(timings),
            'median_render_ms': statistics.median(run['render']['total_ns'] / run['frames'] / 1e6 for run in runs),
            'median_present_ms': statistics.median(run['present']['total_ns'] / run['frames'] / 1e6 for run in runs),
        }
    summary['empty_vs_direct'] = summary['profiles']['empty']['median_frame_ms'] / summary['profiles']['direct']['median_frame_ms']
    destination = args.output / 'summary.json'
    destination.write_text(json.dumps(summary, indent=2) + '\n')
    print(f'Comparison evidence: {destination}')


if __name__ == '__main__':
    main()
