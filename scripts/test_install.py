#!/usr/bin/env python3
"""Exercise the real bash installer against an isolated local release server."""
import contextlib
import functools
import hashlib
import http.server
import json
import os
from pathlib import Path
import platform
import subprocess
import tarfile
import tempfile
import threading

REPO = Path(__file__).resolve().parents[1]
BINARY = REPO / 'target/debug/honeycomb'


def run(args, env, success=True):
    result = subprocess.run(args, env=env, text=True, capture_output=True, timeout=120)
    if success and result.returncode:
        raise AssertionError(f'{args}: {result.stdout}\n{result.stderr}')
    if not success and result.returncode == 0:
        raise AssertionError(f'{args}: unexpectedly succeeded')
    return result


class QuietHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *_args):
        pass


def main():
    with tempfile.TemporaryDirectory(prefix='honeycomb-installer-e2e-') as tmp:
        root = Path(tmp)
        releases = root / 'releases'
        releases.mkdir()
        target = ('macos' if platform.system() == 'Darwin' else 'linux') + '-' + ('aarch64' if platform.machine() in ('arm64', 'aarch64') else 'x86_64')
        archive = releases / f'honeycomb-{target}.tar.gz'
        with tarfile.open(archive, 'w:gz') as tar:
            tar.add(BINARY, arcname='honeycomb')
        checksum = archive.with_name(archive.name + '.sha256')
        checksum.write_text(hashlib.sha256(archive.read_bytes()).hexdigest() + '  ' + archive.name + '\n')
        (releases / 'install.sh').write_bytes((REPO / 'install.sh').read_bytes())
        handler = functools.partial(QuietHandler, directory=str(releases))
        with http.server.ThreadingHTTPServer(('127.0.0.1', 0), handler) as server:
            thread = threading.Thread(target=server.serve_forever, daemon=True)
            thread.start()
            base = f'http://127.0.0.1:{server.server_port}'
            home = root / 'isolated home'
            home.mkdir()
            env = dict(os.environ, HOME=str(home), SILICON_HOME=str(home), SHELL='/bin/bash', HONEYCOMB_RELEASE_BASE=base,
                       HONEYCOMB_ALLOW_LOCAL='1', HONEYCOMB_NO_SERVICE='1')
            env.pop('ZDOTDIR', None)
            env.pop('HONEYCOMB_NO_MODIFY_PATH', None)
            (home / '.bashrc').write_text('# existing shell configuration\n')
            # The exact curl -> bash invocation requested by the user, against a local fixture.
            run(['/bin/bash', '-c', f'/bin/bash -c "$(curl -fsSL {base}/install.sh)"'], env)
            installed = home / '.honeycomb/dir/system/bin/honeycomb'
            assert installed.is_file()
            assert (home / '.bashrc').read_text().startswith('# existing shell configuration\n')
            bashrc = (home / '.bashrc').read_text()
            run(['/bin/bash', str(REPO / 'install.sh')], env)
            assert (home / '.bashrc').read_text() == bashrc
            assert run(['/bin/bash', '--noprofile', '--rcfile', str(home / '.bashrc'), '-ic', 'command -v honeycomb'], env).stdout.strip() == str(installed)
            assert run(['/bin/bash', '-lc', 'command -v honeycomb'], env).stdout.strip() == str(installed)
            if Path('/bin/zsh').exists():
                zdot = home / 'zsh config'
                zenv = dict(env, SHELL='/bin/zsh', ZDOTDIR=str(zdot))
                run(['/bin/bash', str(REPO / 'install.sh')], zenv)
                zshrc = (zdot / '.zshrc').read_text()
                run(['/bin/bash', str(REPO / 'install.sh')], zenv)
                assert (zdot / '.zshrc').read_text() == zshrc
                assert run(['/bin/zsh', '-ic', 'command -v honeycomb'], zenv).stdout.strip() == str(installed)
            untouched = home / 'no-shell-changes'
            untouched.mkdir()
            run(['/bin/bash', str(REPO / 'install.sh')], dict(env, HOME=str(untouched), HONEYCOMB_NO_MODIFY_PATH='1'))
            assert not (untouched / '.bashrc').exists()
            assert run([str(installed), '--version'], env).stdout.startswith('honeycomb ')
            run([str(installed), 'config', 'set', 'auto_update', 'false'], env)
            status = json.loads(run([str(installed), 'login', 'status', '--json'], env).stdout)
            assert status == {'authenticated': False}
            help_text = run([str(installed), '--help'], env).stdout
            for command in ['pack', 'validate', 'install', 'uninstall', 'update', 'environments', 'service']:
                assert command in help_text
            shell = run([str(installed), 'config', 'env'], env).stdout
            run(['/bin/bash', '-c', shell + '\ncommand -v honeycomb'], env)
            before = installed.read_bytes()
            checksum.write_text('0' * 64 + '  ' + archive.name + '\n')
            failure = run(['/bin/bash', str(REPO / 'install.sh')], env, success=False)
            assert 'verification failed' in failure.stderr
            assert installed.read_bytes() == before
            # Existing-directory validation and persisted home selection.
            run([str(installed), 'config', 'home', str(root / 'missing')], env, success=False)
            alternative = root / 'other home'
            alternative.mkdir()
            run([str(installed), 'config', 'home', str(alternative)], env)
            changed = json.loads(run([str(installed), 'config', 'show'], env).stdout)
            assert changed['data_directory'] == str(alternative.resolve() / '.honeycomb/dir')
            server.shutdown()
            print('PASS: curl/bash installation, checksum verification, executable/help, unauthenticated status, shell setup, corrupted-release rejection, isolated home configuration')


if __name__ == '__main__':
    main()
