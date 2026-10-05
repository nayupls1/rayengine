#!/usr/bin/env python3
"""Unix setup regression tests. No real home or installed Rust is modified."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

REPO = Path(__file__).resolve().parents[2]
SETUP = REPO / 'scripts/setup.sh'


class SetupTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='rayengine setup ')
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name).resolve()
        self.home = self.base / 'home with spaces'
        self.home.mkdir()
        self.tools = self.base / 'tools'
        self.tools.mkdir()
        self.root = self.base / "cli 'quoted' $literal `literal` [glob]"
        self.log = self.base / 'cargo.json'
        self.env = dict(os.environ, HOME=str(self.home), SHELL='/bin/bash',
                        PATH=f'{self.tools}:/usr/bin:/bin', MOCK_LOG=str(self.log))
        for key in ('CARGO_HOME', 'CARGO_INSTALL_ROOT', 'ZDOTDIR', 'XDG_CONFIG_HOME'):
            self.env.pop(key, None)
        self.tool('rustc', '''#!/bin/sh
if [ "$1" = '-vV' ]; then echo 'host: test-host'; else echo "rustc ${MOCK_RUST_VERSION:-1.89.0} (fixture)"; fi
''')
        self.tool('cargo', f'''#!{shutil.which('python3')}
import json, os, pathlib, sys
if sys.argv[1] == '--version':
    print('cargo 1.89.0'); sys.exit(0)
pathlib.Path(os.environ['MOCK_LOG']).write_text(json.dumps(sys.argv[1:]))
if os.environ.get('MOCK_FAIL'): sys.exit(17)
if os.environ.get('MOCK_EDIT_CONFIG'):
    with (pathlib.Path(os.environ['HOME']) / '.bashrc').open('a') as f:
        f.write('export MY_NEW_SETTING=keep_me\\n')
root = pathlib.Path(sys.argv[sys.argv.index('--root') + 1])
binary = root / 'bin/rayengine'
binary.parent.mkdir(parents=True, exist_ok=True)
if not os.environ.get('MOCK_NO_BINARY'):
    binary.write_text('#!/bin/sh\\necho "rayengine fixture"\\n')
    binary.chmod(0o755)
''')

    def tool(self, name, content):
        path = self.tools / name
        path.write_text(content)
        path.chmod(0o755)

    def setup(self, *args, ok=True):
        result = subprocess.run(['bash', str(SETUP), *args], cwd=self.base,
                                env=self.env, text=True, capture_output=True)
        if ok:
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        else:
            self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        return result

    def install(self, *args, **kwargs):
        return self.setup('--root', str(self.root), *args, **kwargs)

    def fresh(self, shell='bash', startup=None, extra_env=None):
        env = dict(self.env)
        env.update(extra_env or {})
        command = 'rayengine --help; printf "\\nPATH_RESULT=%s\\n" "$PATH"'
        if shell == 'bash':
            args = ['bash', '--noprofile', '-ic', command]
        elif shell == 'sh':
            args = ['sh', '-c', f'. "$HOME/.profile"; {command}']
        elif shell == 'fish':
            args = [shutil.which('fish'), '-ic', 'rayengine --help; printf "\\nPATH_RESULT=%s\\n" (string join : $PATH)']
        else:
            args = [shutil.which('zsh'), '-ic', command]
        if startup:
            args = ['bash', '--noprofile', '--norc', '-c', f'. "{startup}"; {command}']
        result = subprocess.run(args, env=env, text=True, capture_output=True)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn('rayengine fixture', result.stdout, result.stdout + result.stderr)
        value = result.stdout.split('PATH_RESULT=', 1)[1].strip()
        self.assertEqual(value.split(':').count(str(self.root / 'bin')), 1, value)
        return value

    def test_bash_spaces_literals_idempotence_and_reversal(self):
        baseline = b'# unrelated\nexport KEEP_ME=yes'  # no trailing newline
        rc = self.home / '.bashrc'
        rc.write_bytes(baseline)
        rc.chmod(0o640)
        profile = self.home / '.profile'
        profile.write_bytes(b'# login\n\n')
        self.install('--debug')
        first = rc.read_bytes(), profile.read_bytes()
        self.install('--debug')
        self.assertEqual(first, (rc.read_bytes(), profile.read_bytes()))
        self.assertEqual(rc.stat().st_mode & 0o777, 0o640)
        args = json.loads(self.log.read_text())
        self.assertEqual(args[args.index('--path') + 1], str(REPO / 'crates/rayengine-cli'))
        self.assertEqual(args[args.index('--root') + 1], str(self.root))
        self.assertIn('--locked', args)
        self.assertIn('--debug', args)
        self.assertEqual(args[args.index('--target') + 1], 'test-host')
        self.fresh()
        self.fresh(extra_env={'PATH': f'{self.tools}:/usr/bin:{self.root}/bin:/bin'})
        self.fresh(startup=profile)
        self.setup('--remove-path')
        self.assertEqual(rc.read_bytes(), baseline)
        self.assertEqual(profile.read_bytes(), b'# login\n\n')
        self.setup('--remove-path')
        self.assertEqual(rc.read_bytes(), baseline)
        self.assertTrue((self.root / 'bin/rayengine').is_file())

    def test_current_path_is_not_evidence_of_persistence(self):
        self.env['PATH'] += f':{self.root}/bin'
        self.install()
        self.fresh(extra_env={'PATH': f'{self.tools}:/usr/bin:/bin'})

    def test_custom_cargo_home_and_install_root_precedence(self):
        cargo_home = self.base / 'cargo home'
        cargo_home.mkdir()
        (cargo_home / 'config.toml').write_text('[install]\nroot = "/ignored/config/root"\n')
        self.env['CARGO_HOME'] = str(cargo_home)
        self.setup()
        self.assertTrue((cargo_home / 'bin/rayengine').exists())
        env_root = self.base / 'env install'
        self.env['CARGO_INSTALL_ROOT'] = str(env_root)
        self.setup()
        self.assertTrue((env_root / 'bin/rayengine').exists())
        self.install()
        self.assertTrue((self.root / 'bin/rayengine').exists())
        rc = (self.home / '.bashrc').read_text()
        self.assertEqual(rc.count('# >>> rayengine'), 1)
        self.assertNotIn(str(env_root / 'bin'), rc)

    def test_default_and_relative_root(self):
        self.setup()
        self.assertTrue((self.home / '.cargo/bin/rayengine').exists())
        self.setup('--root', 'relative install')
        self.assertTrue((self.base / 'relative install/bin/rayengine').exists())

    def test_bash_login_precedence_and_symlink_preservation(self):
        target = self.home / 'actual profile'
        target.write_text('# preserved\n')
        profile = self.home / '.bash_profile'
        profile.symlink_to(target)
        login = self.home / '.bash_login'
        login.write_text('# untouched\n')
        self.install()
        self.assertTrue(profile.is_symlink())
        self.assertIn('# >>> rayengine', target.read_text())
        self.assertEqual(login.read_text(), '# untouched\n')
        self.assertFalse((self.home / '.profile').exists())
        self.fresh(startup=profile)
        self.setup('--remove-path')
        self.assertEqual(target.read_text(), '# preserved\n')

    def test_sh_profile(self):
        self.install('--shell', 'sh')
        self.fresh('sh')
        self.setup('--shell', 'sh', '--remove-path')
        self.assertEqual((self.home / '.profile').read_text(), '')

    @unittest.skipUnless(shutil.which('zsh'), 'zsh unavailable')
    def test_zsh_and_custom_zdotdir(self):
        dotdir = self.base / 'zsh config'
        self.env['ZDOTDIR'] = str(dotdir)
        self.install('--shell', 'zsh')
        self.fresh('zsh')
        self.assertTrue((dotdir / '.zprofile').exists())
        result = subprocess.run([shutil.which('zsh'), '-lc', 'rayengine --help'], env=self.env, text=True, capture_output=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.setup('--shell', 'zsh', '--remove-path')
        self.assertEqual((dotdir / '.zshrc').read_text(), '')

    @unittest.skipUnless(shutil.which('fish'), 'fish unavailable')
    def test_fish_and_custom_config_home(self):
        config = self.base / 'config home'
        self.env['XDG_CONFIG_HOME'] = str(config)
        self.install('--shell', 'fish')
        file = config / 'fish/conf.d/rayengine-cli.fish'
        first = file.read_bytes()
        self.install('--shell', 'fish')
        self.assertEqual(file.read_bytes(), first)
        self.fresh('fish')
        self.setup('--shell', 'fish', '--remove-path')
        self.assertEqual(file.read_text(), '')

    def test_failed_install_does_not_edit_configuration(self):
        for failure in ('MOCK_FAIL', 'MOCK_NO_BINARY'):
            with self.subTest(failure=failure):
                self.env[failure] = '1'
                self.install(ok=False)
                self.assertFalse((self.home / '.bashrc').exists())
                self.env.pop(failure)

    def test_old_or_missing_rust_and_unsupported_shell(self):
        self.env['MOCK_RUST_VERSION'] = '1.88.0'
        self.assertIn('rustup update stable', self.install(ok=False).stderr)
        (self.tools / 'rustc').unlink()
        self.assertIn('rustup.rs', self.install(ok=False).stderr)
        self.assertIn('Unsupported shell', self.install('--shell', 'nu', ok=False).stderr)
        self.assertFalse(self.log.exists())

    def test_invalid_root_and_malformed_block(self):
        self.setup('--root', '/bad:root', ok=False)
        rc = self.home / '.bashrc'
        original = '# >>> rayengine CLI PATH >>>\nmissing end'
        rc.write_text(original)
        self.install(ok=False)
        self.assertEqual(rc.read_text(), original)
        self.assertFalse(self.log.exists())

    def test_preserve_configuration_edits_during_install(self):
        self.env['MOCK_EDIT_CONFIG'] = '1'
        self.install()
        rc = self.home / '.bashrc'
        self.assertIn('export MY_NEW_SETTING=keep_me', rc.read_text())
        self.setup('--remove-path')
        self.assertEqual(rc.read_text(), 'export MY_NEW_SETTING=keep_me\n')

    def test_resolved_root_must_be_representable_in_path(self):
        physical = self.base / 'unsafe:physical'
        physical.mkdir()
        alias = self.base / 'safe alias'
        alias.symlink_to(physical, target_is_directory=True)
        result = self.setup('--root', str(alias), ok=False)
        self.assertIn('Resolved install root', result.stderr)
        self.assertFalse(self.log.exists())
        self.assertFalse((self.home / '.bashrc').exists())

    def test_remove_path_requires_no_toolchain(self):
        self.install()
        (self.tools / 'rustc').unlink()
        (self.tools / 'cargo').unlink()
        self.setup('--remove-path')
        self.assertEqual((self.home / '.bashrc').read_text(), '')


if __name__ == '__main__':
    unittest.main()
