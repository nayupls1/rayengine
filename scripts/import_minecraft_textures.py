#!/usr/bin/env python3
"""Extract the demo's PNG subset once; the Rust game only reads ordinary files."""
import argparse
import json
from pathlib import Path
import shutil
import struct
import zipfile

FILES = (
    'bedrock', 'stone', 'dirt', 'grass_block_top', 'coal_ore', 'iron_ore',
    'oak_log', 'oak_leaves', 'grass_block_side', 'oak_log_top',
    'grass_block_side_overlay',
)


def extract(source: Path, output: Path) -> None:
    """Validate selected entries before writing into a new output directory."""
    if source.stat().st_size > 512 * 1024 * 1024:
        raise ValueError('client/resource archive exceeds 512 MiB')
    if output.exists():
        raise ValueError(f'{output}: already exists; choose a new directory')
    images = {}
    with zipfile.ZipFile(source) as archive:
        if len(archive.infolist()) > 100_000:
            raise ValueError('archive exceeds 100,000 entries')
        for name in FILES:
            entry = f'assets/minecraft/textures/block/{name}.png'
            info = archive.getinfo(entry)
            if info.file_size > 4 * 1024 * 1024:
                raise ValueError(f'{entry}: exceeds 4 MiB')
            with archive.open(info) as file:
                data = file.read(4 * 1024 * 1024 + 1)
            if len(data) > 4 * 1024 * 1024 or len(data) < 33 or data[:8] != b'\x89PNG\r\n\x1a\n' or data[12:16] != b'IHDR':
                raise ValueError(f'{entry}: unsupported PNG header')
            width, height = struct.unpack('>II', data[16:24])
            if width != height or not 16 <= width <= 256 or width & (width - 1):
                raise ValueError(f'{entry}: expected static square PNG, 16..256 pixels')
            images[f'{name}.png'] = data
    output.mkdir(parents=True, exist_ok=False)
    try:
        for name, data in images.items():
            with (output / name).open('xb') as file:
                file.write(data)
    except BaseException:
        shutil.rmtree(output)
        raise


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=Path, help='explicit Java client JAR or resource-pack ZIP')
    parser.add_argument('--output', type=Path, default=Path(__file__).resolve().parents[1] / 'examples/minecraft/local-assets/minecraft', help='new directory; default is ignored by Git')
    args = parser.parse_args()
    try:
        extract(args.source, args.output)
    except (OSError, ValueError, KeyError, RuntimeError, zipfile.BadZipFile) as error:
        parser.exit(1, f'{args.source}: {error}\n')
    print(json.dumps({'source': str(args.source), 'output': str(args.output), 'files': [f'{name}.png' for name in FILES]}, indent=2))


if __name__ == '__main__':
    main()
