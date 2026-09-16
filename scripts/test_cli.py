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
            for role in ['owner', 'member', 'outsider', 'validator', 'anonymous']:
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
            webhook = run('apps', 'webhook', 'status', 'tos>briefcase')
            proof = root / 'webhook-proof'; proof.write_text('fixture-webhook-proof')
            secret_file = root / 'webhook-secret'; secret_file.write_text('replacement-fixture-webhook-signing-secret')
            pending_webhook = run('apps', 'webhook', 'approve', 'tos>briefcase', '--endpoint', webhook['pending_endpoint_id'], '--revision', '1')
            assert pending_webhook['error_code'] == 'step_up_required'
            accepted_webhook = run('apps', 'webhook', 'retry', pending_webhook['id'], '--step-up-file', str(proof))
            assert accepted_webhook['state'] == 'accepted'
            rotated_webhook = run('apps', 'webhook', 'rotate-secret', 'tos>briefcase', '--revision', '1', '--secret-file', str(secret_file), '--step-up-file', str(proof))
            assert rotated_webhook['state'] == 'accepted'
            assert 'replacement-fixture-webhook' not in json.dumps(rotated_webhook)
            cleanup = run('environments', 'retention', 'fixture-cleanup-desktop')['automatic_cleanup']
            assert cleanup['state'] == 'pending' and cleanup['kind'] == 'environment.retire'
            assert cleanup['applications'] == ['tos>briefcase']
            retention_id = 'fixture-retention-desktop'
            assert run('environments', 'retention', retention_id)['idle_days'] == 30
            assert run('environments', 'set-retention', retention_id, '--days', '60', '--revision', '1')['revision'] == 2
            activity = run('environments', 'activity', retention_id, 'tos>briefcase', '--generation', '1', '--key-version', '1')
            assert activity['replayed'] is False
            assert run('environments', 'retention', retention_id)['idle_days'] == 60
            logo_file = REPO / 'web/tests/fixtures/logo.png'
            run('apps', 'upload-logo', 'tos', str(logo_file), role='member', ok=False)
            logo = run('--idempotency-key', 'cli-logo-upload-0001', 'apps', 'upload-logo', 'tos', str(logo_file))
            assert logo['state'] == 'accepted' and logo['logo_url'].startswith('https://briefcase.fixture.invalid/')
            assert run('--idempotency-key', 'cli-logo-upload-0001', 'apps', 'upload-logo', 'tos', str(logo_file)) == logo
            app_id = 'tos>cli-e2e'
            app = {'org_id': 'tos', 'local_app_id': 'cli-e2e', 'name': 'CLI integration',
                   'description': 'This application exercises the complete local Honeycomb release installation workflow. ' * 7,
                   'webhook_scope': ['membership'], 'webhook_url': 'https://example.com/webhook/', 'webhook_secret': 'test-signing-secret-000000000000000000'}
            app['logo_url'] = logo['logo_url']
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
            run('publication', 'decide', request['id'], 'honeycomb', 'approve', '--revision', '1', role='validator')
            published = run('publication', 'activate', request['id'], '--revision', '1')
            assert published['state'] == 'accepted'
            assert run('apps', 'get', app_id, role='anonymous')['visibility'] == 'public'
            public_install = run('install', app_id, role='anonymous')
            assert public_install['version'] == '1.1.0'
            public_executable = Path(public_install['bin_directory']) / 'honeycomb-e2e-greet'
            assert subprocess.check_output([str(public_executable)], text=True).strip() == 'hello-1.1.0'
            run('uninstall', app_id, role='anonymous')
            run('uninstall', app_id, role='member')
            assert not executable.exists()
            assert not run('installed', role='member')
            inbox = run('publication', 'inbox')
            assert any(r['id'] == 'fixture-review-desktop' for r in inbox['items'])
            run('publication', 'review-reply', 'fixture-review-desktop', 'tos>briefcase', '--message', 'The requested scopes have been reviewed.')
            decision = run('publication', 'decide', 'fixture-review-desktop', 'tos>briefcase', 'approve', '--reason', 'Appropriate declared file access.', '--revision', '1')
            assert decision['publication_state'] == 'awaiting_validator'
            reconciled = run('apps', 'reconcile', 'tos>briefcase')
            assert reconciled['state'] == 'pending' and 'not yet available' in reconciled['error']
            rotation = run('apps', 'rotate-secret', 'tos>briefcase', '--revision', '1')
            assert rotation['state'] == 'pending' and rotation['error_code'] == 'integration_unavailable'
            imported = run('environments', 'import', 'fixture-import-desktop', 'tos>briefcase', '--revision', '1')
            assert imported['operation_state'] == 'pending' and imported['state'] == 'ready'
            report = run('report', 'The local test report reproduces a CLI issue.', role='member')
            assert report['saved'] and report['notification'] == 'pending'
            run('logout', role='member')
            assert run('login', 'status', role='member') == {'authenticated': False}
            print('PASS: CLI IAM discovery/login, logo upload/replay, webhook approval/step-up retry/signing rotation, retention/activity, role gates, idempotency, private visibility, six-target validation/pack, immutable uploads, install/execute/update/aliases, metrics/review/star, publication review/activation/anonymous install, dependency import progress, reports, uninstall/logout')
        finally:
            fixture.terminate()
            fixture.wait(timeout=10)


if __name__ == '__main__':
    main()
