#!/usr/bin/env python3
"""Real CLI/HTTP/database journey with explicit IAM and archive test doubles."""
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time
import urllib.request

REPO = Path(__file__).resolve().parents[1]
CLI = REPO / 'target/debug/honeycomb'
TARGETS = ['linux-x86_64', 'linux-aarch64', 'windows-x86_64', 'windows-aarch64', 'macos-x86_64', 'macos-aarch64']


def main():
    with tempfile.TemporaryDirectory(prefix='honeycomb-cli-e2e-') as tmp:
        root = Path(tmp)
        with socket.socket() as sock:
            sock.bind(('127.0.0.1', 0))
            port = sock.getsockname()[1]
        api = f'http://127.0.0.1:{port}'
        fixture = subprocess.Popen([str(REPO / 'target/debug/examples/e2e_fixture')], env=dict(os.environ, HONEYCOMB_FIXTURE_PORT=str(port)), stdout=subprocess.DEVNULL)
        try:
            for _ in range(100):
                try:
                    urllib.request.urlopen(api + '/health', timeout=1).close()
                    break
                except OSError:
                    time.sleep(.1)
            else:
                raise AssertionError('Fixture did not start')
            homes = {}
            for role in ['owner', 'member', 'outsider', 'anonymous']:
                home = root / role
                home.mkdir()
                homes[role] = dict(os.environ, SILICON_HOME=str(home), HONEYCOMB_API_URL=api)

            def run(*args, role='owner', ok=True, structured=True):
                command = [str(CLI), '--json', *args]
                result = subprocess.run(command, env=homes[role], capture_output=True, text=True, timeout=30)
                if ok and result.returncode:
                    raise AssertionError(f'{args}: {result.stderr}\n{result.stdout}')
                if not ok:
                    assert result.returncode, f'{args} unexpectedly succeeded'
                    return result
                return json.loads(result.stdout) if structured else result.stdout

            for role in homes:
                run('config', 'set', 'auto_update', 'false', role=role)
                if role != 'anonymous':
                    assert run('login', 'fixture-' + role, role=role)['authenticated']
            assert run('login', 'status', role='anonymous') == {'authenticated': False}
            assert run('iam')['app_id'] == 'tos>honeycomb'
            app_id = 'tos>cli-e2e'
            app = {'org_id': 'tos', 'local_app_id': 'cli-e2e', 'name': 'CLI integration',
                   'description': 'This application exercises the complete local Honeycomb release installation workflow. ' * 7,
                   'webhook_url': 'https://example.com/webhook/', 'webhook_secret': 'test-signing-secret-000000000000000000'}
            config = root / 'application.json'
            config.write_text(json.dumps(app))
            run('apps', 'create', str(config), role='member', ok=False)
            key = 'cli-create-e2e-0001'
            operation = run('--idempotency-key', key, 'apps', 'create', str(config))
            assert operation['state'] == 'accepted'
            assert run('--idempotency-key', key, 'apps', 'create', str(config))['id'] == operation['id']
            for role in ['outsider', 'anonymous']:
                assert run('search', 'CLI integration', role=role)['total'] == 0
                run('apps', 'get', app_id, role=role, ok=False)
            assert run('apps', 'get', app_id, role='member')['visibility'] == 'private'
            package = root / 'package'
            package.mkdir()
            targets = {}
            for target in TARGETS:
                path = package / 'targets' / target / 'bin' / 'greet'
                path.parent.mkdir(parents=True)
                targets[target] = {'root': 'targets/' + target, 'executables': {'app': 'bin/greet'}}
            for version in ['1.0.0', '1.1.0']:
                for target in TARGETS:
                    path = package / 'targets' / target / 'bin' / 'greet'
                    path.write_text(f'#!/bin/sh\nprintf "hello-{version}\\n"\n')
                    path.chmod(0o755)
                manifest = {'format_version': 1, 'app_id': app_id, 'version': version,
                            'bin': {'honeycomb-e2e-greet': 'app'}, 'targets': targets}
                (package / 'honeycomb.yaml').write_text(json.dumps(manifest))
                assert run('validate', str(package))['valid']
                archive = root / f'{version}.tar.gz'
                run('pack', str(package), '--output', str(archive))
                upload = run('--idempotency-key', 'cli-upload-e2e-' + version, 'releases', 'upload', app_id, str(archive), '--revision', '1')
                assert upload['state'] == 'accepted'
                # A version cannot be overwritten with another upload operation.
                run('releases', 'upload', app_id, str(archive), '--revision', '1', ok=False)
                args = ['install', app_id, '--alias', 'honeycomb-e2e-greet=honeycomb-e2e-custom'] if version == '1.0.0' else ['update', app_id]
                installed = run(*args, role='member')
                assert installed['version'] == version
                executable = Path(installed['bin_directory']) / 'honeycomb-e2e-custom'
                assert subprocess.check_output([str(executable)], text=True).strip() == 'hello-' + version
            run('review', app_id, '--rating', '4.7', '--review', 'The full install journey works.', role='member')
            run('star', app_id, role='member')
            run('star', app_id, role='member')
            details = run('apps', 'get', app_id, role='member')
            assert details['stars'] == 1 and details['reviews'] == 1 and details['rating'] == 4.7
            assert details['installs'] == 2
            request = run('publication', 'request', app_id, '--message', 'Please review this working release.', '--revision', '1')
            assert request
            assert run('apps', 'get', app_id, role='member')['visibility'] == 'private'
            run('uninstall', app_id, role='member')
            assert not executable.exists()
            assert not run('installed', role='member')
            report = run('report', 'The local test report reproduces a CLI issue.', role='member')
            assert report['saved'] and report['notification'] == 'pending'
            run('logout', role='member')
            assert run('login', 'status', role='member') == {'authenticated': False}
            print('PASS: CLI IAM discovery/login, role gates, idempotency, private visibility, six-target validation/pack, immutable uploads, install/execute/update/aliases, metrics/review/star, publication gate, uninstall/logout')
        finally:
            fixture.terminate()
            fixture.wait(timeout=10)


if __name__ == '__main__':
    main()
