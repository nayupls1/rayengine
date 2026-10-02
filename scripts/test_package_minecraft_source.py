#!/usr/bin/env python3
"""Exercise candidate source checks using an isolated Git repository."""
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import package_minecraft


class SourceStatusTests(unittest.TestCase):
    def test_only_private_staging_is_excluded(self):
        with tempfile.TemporaryDirectory(prefix='rayengine-package-source-') as temp:
            root = Path(temp).resolve()
            def git(*args):
                subprocess.run(['git', *args], cwd=root, check=True, capture_output=True)
            git('init', '-q')
            source = root / 'source.rs'
            source.write_text('original\n')
            git('add', 'source.rs')
            git('-c', 'user.name=Test', '-c', 'user.email=test@example.invalid',
                'commit', '-qm', 'fixture')
            staging = root / 'custom-output/.private[1]'
            staging.mkdir(parents=True)
            (staging / 'candidate').write_text('generated\n')
            with patch.object(package_minecraft, 'ROOT', root):
                self.assertTrue(package_minecraft.source_status())
                self.assertEqual(package_minecraft.source_status(staging), '')
                source.write_text('changed\n')
                self.assertIn('source.rs', package_minecraft.source_status(staging))
                source.write_text('original\n')
                sibling = staging.with_name(staging.name + '-other')
                sibling.mkdir()
                (sibling / 'untracked.rs').write_text('source\n')
                self.assertTrue(package_minecraft.source_status(staging))
                self.assertTrue(package_minecraft.source_status(root.parent))


if __name__ == '__main__':
    unittest.main()
