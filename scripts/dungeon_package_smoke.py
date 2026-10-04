#!/usr/bin/env python3
"""Package Embervault with the real CLI; run the extracted archive outside source.
Requires a native display and audio device (CI uses native_audio_smoke.sh).
"""
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[1]
ARTIFACTS = ROOT / 'artifacts/smoke'
ARTIFACTS.mkdir(parents=True, exist_ok=True)
with tempfile.TemporaryDirectory(prefix='embervault-package-') as directory:
    work = Path(directory)
    subprocess.run(['cargo', 'run', '--locked', '-p', 'rayengine-cli', '--', 'package',
                    'examples/games', '--bin', 'dungeon', '--output', str(work / 'bundles')],
                   cwd=ROOT, check=True)
    archive, = (work / 'bundles').glob('*.tar.gz')
    with tarfile.open(archive) as tar:
        tar.extractall(work / 'relocated', filter='data')
    bundle, = (work / 'relocated').iterdir()
    assert len(list((bundle / 'assets').rglob('*.toml'))) == 6
    assert len(list((bundle / 'assets').rglob('room-*.wav'))) == 6
    assert list((bundle / 'fonts').glob('*.font'))
    screenshot = ARTIFACTS / 'dungeon-packaged.png'
    env = dict(os.environ, RAYENGINE_DUNGEON_SAVE=str(work / 'profile.save'))
    # Poison inherited source overrides: launch must install its own manifest.
    env['RAYENGINE_MANIFEST'] = '/nonexistent/source/rayengine.toml'
    env['RAYENGINE_PROFILE'] = 'nonexistent'
    subprocess.run([str(bundle / 'launch'), '--hidden', '--frames', '12',
                    '--screenshot', str(screenshot)], cwd=work, env=env, check=True)
    assert screenshot.read_bytes().startswith(b'\x89PNG\r\n\x1a\n')
    assert (work / 'profile.save').read_bytes().startswith(b'RAYSAVE\0')
    print(f'Embervault relocated bundle passed: {screenshot}')
