#!/usr/bin/env python3
"""Original fixture checks for the one-time extractor; no installed assets needed."""
from pathlib import Path
import struct
import tempfile
import unittest
import zipfile
from import_minecraft_textures import FILES, extract


class ExtractionTests(unittest.TestCase):
    def fixture(self, archive, names=FILES):
        # Enough original PNG header data for the extraction helper's header checks;
        # the Rust image-loader suite covers complete decoding and pixel semantics.
        header = b'\x89PNG\r\n\x1a\n' + struct.pack('>I', 13) + b'IHDR' + struct.pack('>II', 16, 16) + bytes(9)
        with zipfile.ZipFile(archive, 'w', zipfile.ZIP_DEFLATED) as jar:
            for name in names:
                jar.writestr(f'assets/minecraft/textures/block/{name}.png', header)
            jar.writestr('../unexpected.txt', b'never extracted')
        return header

    def test_extracts_only_named_files_and_refuses_overwrite(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            archive, output = root / 'client.jar', root / 'pngs'
            header = self.fixture(archive)
            extract(archive, output)
            self.assertEqual(sorted(p.name for p in output.iterdir()), sorted(f'{name}.png' for name in FILES))
            self.assertEqual((output / 'stone.png').read_bytes(), header)
            self.assertFalse((root / 'unexpected.txt').exists())
            (output / 'stone.png').write_bytes(b'local edit')
            with self.assertRaisesRegex(ValueError, 'already exists'):
                extract(archive, output)
            self.assertEqual((output / 'stone.png').read_bytes(), b'local edit')

    def test_missing_last_texture_does_not_create_partial_directory(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            archive, output = root / 'client.jar', root / 'pngs'
            self.fixture(archive, FILES[:-1])
            with self.assertRaises(KeyError):
                extract(archive, output)
            self.assertFalse(output.exists())


if __name__ == '__main__':
    unittest.main()
