#!/usr/bin/env python3
"""Real checkout install, fresh shell and uninstall on Linux/macOS, in a temp home."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

REPO = Path(__file__).resolve().parents[2]
SETUP = REPO / 'scripts/setup.sh'


def run(*args, env):
    subprocess.run(args, cwd=REPO, env=env, check=True)


with tempfile.TemporaryDirectory(prefix='rayengine real setup ') as temporary:
    directory = Path(temporary)
    home = directory / 'home with spaces'
    home.mkdir()
    root = directory / "cli's install root"
    # Keep Rust's actual toolchain and download cache; only shell config and the
    # explicitly selected install root are isolated.
    env = dict(os.environ, HOME=str(home), SHELL='/bin/bash',
               CARGO_HOME=os.environ.get('CARGO_HOME', str(Path.home() / '.cargo')),
               RUSTUP_HOME=os.environ.get('RUSTUP_HOME', str(Path.home() / '.rustup')),
               CARGO_INSTALL_ROOT=str(root))
    for shell in ('bash', 'zsh', 'fish', 'sh'):
        env['ZDOTDIR'] = str(home / 'zsh config')
        env['XDG_CONFIG_HOME'] = str(home / 'config')
        run('bash', str(SETUP), '--shell', shell, '--debug', env=env)
        first = {p: p.read_bytes() for p in home.rglob('*') if p.is_file()}
        run('bash', str(SETUP), '--shell', shell, '--root', str(root), '--debug', env=env)
        assert first == {p: p.read_bytes() for p in home.rglob('*') if p.is_file()}
        # No inherited CLI PATH: fresh processes must obtain it from startup.
        fresh_env = dict(env, PATH='/usr/local/bin:/usr/bin:/bin:/opt/homebrew/bin',
                         RAYENGINE_EXPECTED_BINARY=str(root / 'bin/rayengine'))
        check = 'test "$(command -v rayengine)" = "$RAYENGINE_EXPECTED_BINARY" && rayengine --help'
        if shell == 'bash':
            run('bash', '--noprofile', '-ic', check, env=fresh_env)
            run('bash', '--noprofile', '--norc', '-c', '. "$HOME/.profile"; ' + check, env=fresh_env)
        elif shell == 'zsh':
            run(shutil.which('zsh'), '-ic', check, env=fresh_env)
            run(shutil.which('zsh'), '-lc', check, env=fresh_env)
        elif shell == 'fish':
            run(shutil.which('fish'), '-ic', 'test (command -s rayengine) = "$RAYENGINE_EXPECTED_BINARY"; and rayengine --help', env=fresh_env)
        else:
            run('sh', '-c', '. "$HOME/.profile"; ' + check, env=fresh_env)
        run('bash', str(SETUP), '--shell', shell, '--remove-path', env=env)
        run('bash', str(SETUP), '--shell', shell, '--remove-path', env=env)
        # Managed files are empty after reversal. Fish may write its own files.
        managed = {'bash': [home / '.bashrc', home / '.profile'],
                   'zsh': [Path(env['ZDOTDIR']) / '.zshrc', Path(env['ZDOTDIR']) / '.zprofile'],
                   'fish': [Path(env['XDG_CONFIG_HOME']) / 'fish/conf.d/rayengine-cli.fish'],
                   'sh': [home / '.profile']}[shell]
        assert all(p.read_bytes() == b'' for p in managed)
    run('cargo', 'uninstall', 'rayengine-cli', '--root', str(root), env=env)
    assert not (root / 'bin/rayengine').exists()
print('Unix install, fresh shells, idempotence and reversal smoke passed.')
