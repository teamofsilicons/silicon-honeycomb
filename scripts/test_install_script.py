#!/usr/bin/env python3
"""Drive the real CLI through install scripts: success, JSON output, failure, opt-out and updates."""
import hashlib
import http.server
import json
import os
from pathlib import Path
import subprocess
import tempfile
import threading

REPO = Path(__file__).resolve().parents[1]
CLI = REPO / 'target/debug/honeycomb'
TARGETS = ['linux-x86_64', 'linux-aarch64', 'macos-x86_64', 'macos-aarch64', 'windows-x86_64', 'windows-aarch64']


def package(root, version, exit_code, env):
    """Pack a release whose install script appends a line to $HONEYCOMB_BIN_DIR/../script-runs."""
    folder = root / f'package-{version}-{exit_code}'
    targets = {}
    for target in TARGETS:
        payload = folder / 'targets' / target
        payload.mkdir(parents=True)
        binary = payload / 'hello'
        binary.write_text('#!/bin/sh\necho hello\n')
        binary.chmod(0o755)
        windows = target.startswith('windows')
        script = payload / ('setup.cmd' if windows else 'setup.sh')
        script.write_text(
            '#!/bin/sh\n'
            'echo "install script says hello"\n'
            'echo "$HONEYCOMB_APP_ID $HONEYCOMB_APP_VERSION $HONEYCOMB_RELEASE_CHANNEL" >> "$HONEYCOMB_BIN_DIR/../script-runs"\n'
            f'exit {exit_code}\n'
        )
        script.chmod(0o755)
        targets[target] = {'root': 'targets/' + target, 'executables': {'main': 'hello'}, 'install_script': script.name}
    manifest = {'format_version': 1, 'version': version, 'bin': {'honeycomb-install-script-fixture': 'main'}, 'targets': targets}
    (folder / 'honeycomb.yaml').write_text(json.dumps(manifest))
    subprocess.run([str(CLI), 'pack', str(folder)], env=env, capture_output=True, text=True, check=True)
    archive = (folder / f'honeycomb-install-script-fixture-{version}.tar.gz').read_bytes()
    release = dict(app_id='install-script-fixture', version=version, sha256=hashlib.sha256(archive).hexdigest(), size=len(archive), created_at=0)
    return release, archive


def main():
    with tempfile.TemporaryDirectory(prefix='honeycomb-install-script-') as tmp:
        root = Path(tmp)
        env = dict(os.environ, SILICON_HOME=str(root / 'packer'), HONEYCOMB_TELEMETRY='false', SHELL='/bin/bash', HONEYCOMB_NO_MODIFY_PATH='1')
        env.pop('ZDOTDIR', None)
        served = {}

        class Handler(http.server.BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass

            def do_GET(self):
                if self.path.split('?')[0].endswith('/releases'):
                    body = json.dumps({'items': [served['release']]}).encode()
                elif '/download?' in self.path:
                    body = served['archive']
                else:
                    self.send_error(404)
                    return
                self.send_response(200)
                self.send_header('Content-Length', str(len(body)))
                self.end_headers()
                self.wfile.write(body)

        def serve(version, exit_code):
            served['release'], served['archive'] = package(root, version, exit_code, env)

        def honeycomb(home, *args):
            local = dict(env, SILICON_HOME=str(root / home))
            subprocess.run([str(CLI), 'config', 'set', 'auto_update', 'false'], env=local, capture_output=True, check=True)
            return subprocess.run([str(CLI), *args], env=local, capture_output=True, text=True, timeout=120)

        def runs(home):
            matches = list((root / home).rglob('script-runs'))
            return matches[0].read_text().splitlines() if matches else []

        with http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler) as server:
            threading.Thread(target=server.serve_forever, daemon=True).start()
            env['HONEYCOMB_API_URL'] = f'http://127.0.0.1:{server.server_port}'

            serve('1.0.0', 0)
            human = honeycomb('human', 'install', 'install-script-fixture')
            assert human.returncode == 0, human.stderr
            assert 'Running install script' in human.stderr, human.stderr
            assert 'install script says hello' in human.stderr, human.stderr
            assert 'install script says hello' not in human.stdout, 'script output leaked onto stdout'
            assert json.loads(human.stdout)['install_script']['status'] == 'succeeded', human.stdout
            assert runs('human') == ['install-script-fixture 1.0.0 prod'], runs('human')

            as_json = honeycomb('json', '--json', 'install', 'install-script-fixture')
            assert as_json.returncode == 0, as_json.stderr
            result = json.loads(as_json.stdout)
            assert result['install_script']['status'] == 'succeeded' and result['install_script']['exit_code'] == 0, result
            assert runs('json') == ['install-script-fixture 1.0.0 prod'], runs('json')

            # Reinstalling the release already present installs nothing, so nothing runs.
            again = honeycomb('json', '--json', 'install', 'install-script-fixture')
            assert again.returncode == 0 and json.loads(again.stdout)['status'] == 'dependency_resolved', again.stdout
            assert len(runs('json')) == 1, runs('json')

            # Updates never run the install script.
            serve('1.0.1', 0)
            updated = honeycomb('json', '--json', 'update', 'install-script-fixture')
            assert updated.returncode == 0, updated.stderr
            assert 'install_script' not in json.loads(updated.stdout), updated.stdout
            assert len(runs('json')) == 1, runs('json')

            skipped = honeycomb('skipped', '--json', 'install', 'install-script-fixture', '--skip-install-script')
            assert skipped.returncode == 0, skipped.stderr
            assert json.loads(skipped.stdout)['install_script']['status'] == 'skipped', skipped.stdout
            assert runs('skipped') == [], runs('skipped')

            serve('1.0.2', 3)
            failed = honeycomb('failed', 'install', 'install-script-fixture')
            assert failed.returncode == 1, failed.stderr
            assert 'Installation/update failed.' not in failed.stderr, failed.stderr
            assert 'Installed Successfully' in failed.stderr, failed.stderr
            assert 'is installed, but its install script' in failed.stderr and 'exit code 3' in failed.stderr, failed.stderr
            record = json.loads(honeycomb('failed', '--json', 'installed').stdout)
            assert record['install-script-fixture']['version'] == '1.0.2', record
    print('install script checks passed')


if __name__ == '__main__':
    main()
