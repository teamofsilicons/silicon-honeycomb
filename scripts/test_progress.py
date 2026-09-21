#!/usr/bin/env python3
"""Verify immediate progress against a paused server, real installs, JSON and failure output."""
import hashlib
import http.server
import json
import os
from pathlib import Path
import queue
import shutil
import subprocess
import tempfile
import threading
import time

REPO = Path(__file__).resolve().parents[1]
CLI = REPO / 'target/debug/honeycomb'


def main():
    with tempfile.TemporaryDirectory(prefix='honeycomb-progress-') as tmp:
        root = Path(tmp)
        package = root / 'package'
        package.mkdir()
        targets = {}
        for target in ['linux-x86_64', 'linux-aarch64', 'macos-x86_64', 'macos-aarch64', 'windows-x86_64', 'windows-aarch64']:
            folder = package / 'targets' / target
            folder.mkdir(parents=True)
            binary = folder / 'hello'
            binary.write_text('#!/bin/sh\necho hello\n')
            binary.chmod(0o755)
            targets[target] = {'root': 'targets/' + target, 'executables': {'main': 'hello'}}
        (package / 'honeycomb.yaml').write_text(json.dumps({'format_version': 1, 'version': '1.0.0', 'bin': {'honeycomb-progress-fixture': 'main'}, 'targets': targets}))
        env = dict(os.environ, SILICON_HOME=str(root / 'packer'), HONEYCOMB_TELEMETRY='false')
        env.pop('ZDOTDIR', None)
        env.pop('HONEYCOMB_NO_MODIFY_PATH', None)
        env['SHELL'] = '/bin/bash'
        subprocess.run([str(CLI), 'config', 'set', 'auto_update', 'false'], env=env, capture_output=True, check=True)
        packed = subprocess.run([str(CLI), 'pack', str(package)], env=env, capture_output=True, text=True)
        assert packed.returncode == 0, packed.stderr
        archive = (package / 'honeycomb-progress-fixture-1.0.0.tar.gz').read_bytes()
        release = dict(app_id='tos>progress-fixture', version='1.0.0', sha256=hashlib.sha256(archive).hexdigest(), size=len(archive), created_at=0)
        gate = threading.Event()
        requested = threading.Event()

        class Handler(http.server.BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass

            def do_GET(self):
                if self.path.split('?')[0].endswith('/releases'):
                    requested.set()
                    assert gate.wait(10), 'Test never released response gate'
                    body = json.dumps({'items': [release]}).encode()
                elif '/download?' in self.path:
                    body = archive
                else:
                    self.send_error(404)
                    return
                self.send_response(200)
                self.send_header('Content-Length', str(len(body)))
                self.end_headers()
                if '/download?' in self.path:
                    split = len(body) // 2
                    self.wfile.write(body[:split])
                    self.wfile.flush()
                    time.sleep(1.1)
                    self.wfile.write(body[split:])
                else:
                    self.wfile.write(body)

        with http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler) as server:
            threading.Thread(target=server.serve_forever, daemon=True).start()
            env['HONEYCOMB_API_URL'] = f'http://127.0.0.1:{server.server_port}'
            for mode in ['human', 'json', 'corrupt']:
                env['SILICON_HOME'] = str(root / mode)
                subprocess.run([str(CLI), 'config', 'set', 'auto_update', 'false'], env=env, capture_output=True, check=True)
                if mode == 'corrupt':
                    release['sha256'] = '0' * 64
                command = [str(CLI), *(['--json'] if mode == 'json' else []), 'install', release['app_id']]
                process = subprocess.Popen(command, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
                first = ''
                if mode == 'human':
                    lines = queue.Queue()
                    threading.Thread(target=lambda: lines.put(process.stderr.readline()), daemon=True).start()
                    try:
                        first = lines.get(timeout=3)
                        assert 'Starting installation' in first, first
                        assert requested.wait(3), 'Release request did not arrive'
                        assert process.poll() is None, 'Install finished before release response'
                    finally:
                        gate.set()
                stdout, stderr = process.communicate(timeout=15)
                stderr = first + stderr
                assert '\x1b' not in stderr, 'Redirected output must not contain ANSI controls'
                if mode == 'corrupt':
                    assert process.returncode != 0 and 'integrity verification failed' in stderr, stderr
                    assert 'Done.' not in stderr
                    assert 'Installed Successfully' not in stderr
                    assert 'to access it.' not in stderr
                    assert not list((root / mode).rglob('installed.json'))
                    assert not (root / mode / '.bashrc').exists()
                else:
                    assert process.returncode == 0, stderr
                    assert json.loads(stdout)['status'] == 'installed'
                    if mode == 'json':
                        assert stderr == '', stderr
                    else:
                        for text in ['Finding the requested release', 'Downloading: 0%', 'Downloading: 100%', 'Verifying package', 'Unpacking and activating', 'Saving installation record', 'Installed Successfully', 'Run `honeycomb-progress-fixture` to access it.']:
                            assert text in stderr, (text, stderr)
            # Completion uses the actual alias and preserves it on repeated installs.
            release['sha256'] = hashlib.sha256(archive).hexdigest()
            env['SILICON_HOME'] = str(root / 'aliased')
            subprocess.run([str(CLI), 'config', 'set', 'auto_update', 'false'], env=env, capture_output=True, check=True)
            command = [str(CLI), 'install', release['app_id'], '--alias', 'honeycomb-progress-fixture=honeycomb-chosen-alias']
            for status in ['Installed Successfully', 'Dependency resolved.']:
                result = subprocess.run(command, env=env, capture_output=True, text=True, timeout=15)
                assert result.returncode == 0, result.stderr
                assert status in result.stderr, result.stderr
                assert result.stderr.endswith('Run `honeycomb-chosen-alias` to access it.\n'), result.stderr
                json.loads(result.stdout)
            result = subprocess.run([str(CLI), '--json', 'install', release['app_id']], env=env, capture_output=True, text=True, timeout=15)
            assert result.returncode == 0 and result.stderr == '', result.stderr
            resolved = json.loads(result.stdout)
            assert resolved['status'] == 'dependency_resolved', resolved
            assert resolved['bin_directory'] and resolved['help_command'], resolved
            # A real app install repairs shell setup, including installs that already exist.
            for shell in ['bash', *(['zsh'] if shutil.which('zsh') else [])]:
                home = root / (shell + " space ' $dollar `literal`")
                home.mkdir()
                shell_env = dict(env, HOME=str(home), SILICON_HOME=str(home), SHELL='/bin/' + shell)
                shell_env['HONEYCOMB_AUTO_UPDATE'] = '0'
                rc = home / ('.bashrc' if shell == 'bash' else '.zshrc')
                if shell == 'zsh':
                    zdot = home / 'custom-zdot'
                    zdot.mkdir()
                    shell_env['ZDOTDIR'] = str(zdot)
                    rc = zdot / '.zshrc'
                else:
                    (home / '.bash_profile').write_text('# existing login configuration')
                rc.write_text('# existing configuration without a newline')
                args = [str(CLI), '--json', 'install', release['app_id']]

                def installed(extra=(), environment=None):
                    result = subprocess.run(args + list(extra), env=environment or shell_env,
                                            capture_output=True, text=True, timeout=15)
                    assert result.returncode == 0 and not result.stderr, result.stderr
                    return json.loads(result.stdout)

                result = installed()
                assert result['path_setup']['status'] == 'configured', result
                assert result['path_setup']['activation_command'], result
                assert rc.read_text().startswith('# existing configuration without a newline\n')
                before = rc.read_text()
                installed()
                assert rc.read_text() == before, 'Repeated install duplicated shell configuration'
                if shell == 'bash':
                    assert '.honeycomb/dir/app-env' in (home / '.bash_profile').read_text()
                    startup = ['/bin/bash', '--noprofile', '--rcfile', str(rc), '-ic']
                else:
                    startup = ['/bin/zsh', '-ic']
                command = subprocess.run(startup + ['honeycomb-progress-fixture'], env=shell_env,
                                         capture_output=True, text=True, timeout=15)
                assert command.returncode == 0 and command.stdout.strip() == 'hello', command
                activated = dict(shell_env, PATH=result['bin_directory'] + os.pathsep + shell_env['PATH'])
                assert installed(environment=activated)['path_setup']['activation_command'] is None
                # Removing startup configuration is repaired by the already-installed path.
                rc.unlink()
                installed()
                assert rc.read_text().count('# Honeycomb app commands') == 1
                # An unusable startup file must not turn a successful install into a false failure.
                rc.unlink()
                rc.mkdir()
                assert installed()['path_setup']['status'] == 'manual'
                assert Path(result['help_command'].removesuffix(' --help')).exists()
            for mode in ['opt-out', 'test-context']:
                home = root / mode
                home.mkdir()
                isolated = dict(env, HOME=str(home), SILICON_HOME=str(home), HONEYCOMB_AUTO_UPDATE='0')
                extra = []
                if mode == 'opt-out':
                    isolated['HONEYCOMB_NO_MODIFY_PATH'] = '1'
                else:
                    extra = ['--test', 'A' * 32]
                result = subprocess.run([str(CLI), '--json', *extra, 'install', release['app_id']],
                                        env=isolated, capture_output=True, text=True, timeout=15)
                assert result.returncode == 0 and not result.stderr, result.stderr
                assert json.loads(result.stdout)['path_setup']['status'] == 'skipped'
                assert not (home / '.bashrc').exists()
                assert not (home / '.honeycomb/dir/app-env').exists()
            server.shutdown()
    print('PASS: install progress, JSON silence, corrupt-download failure, automatic Bash/Zsh PATH setup, quoted homes, aliases, repeat-install repair, opt-out, test isolation, and startup-file errors')


if __name__ == '__main__':
    main()
