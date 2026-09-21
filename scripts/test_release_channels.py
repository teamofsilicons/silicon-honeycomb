#!/usr/bin/env python3
"""Exercise channel selection, switches, coexistence, and the actual minute daemon."""
import hashlib
import http.server
import json
import os
from pathlib import Path
import pty
import select
import subprocess
import sys
import tempfile
import threading
import time
import urllib.parse

REPO = Path(__file__).resolve().parents[1]
CLI = REPO / 'target/debug/honeycomb'
APP = 'tos>channel-fixture'
COMMAND = 'honeycomb-channel-fixture'


def main():
    with tempfile.TemporaryDirectory(prefix='honeycomb-channels-') as tmp:
        home = Path(tmp)
        env = dict(os.environ, HOME=str(home), SILICON_HOME=str(home),
                   HONEYCOMB_AUTO_UPDATE='0', HONEYCOMB_TELEMETRY='0',
                   HONEYCOMB_NO_SERVICE='1', HONEYCOMB_NO_MODIFY_PATH='1')
        archives = {}
        releases = {'prod': [], 'dev': []}
        requests = []

        def run(*args, ok=True, environment=None):
            p = subprocess.run([str(CLI), '--json', *args], env=environment or env,
                               capture_output=True, text=True, timeout=20)
            assert (p.returncode == 0) == ok, (args, p.stdout, p.stderr)
            if not ok:
                return p
            assert not p.stderr, p.stderr
            return json.loads(p.stdout)

        run('config', 'set', 'auto_update', 'false')

        for channel in releases:
            for version in ['1.0.0', '2.0.0']:
                root = home / channel / version
                targets = {}
                for target in ['linux-x86_64', 'linux-aarch64', 'macos-x86_64', 'macos-aarch64', 'windows-x86_64', 'windows-aarch64']:
                    folder = root / 'targets' / target
                    folder.mkdir(parents=True)
                    binary = folder / 'hello'
                    binary.write_text(f'#!/bin/sh\necho {channel}-{version}\n')
                    binary.chmod(0o755)
                    targets[target] = {'root': 'targets/' + target, 'executables': {'app': 'hello'}}
                (root / 'honeycomb.yaml').write_text(json.dumps(dict(format_version=1, app_id=APP,
                    version=version, bin={COMMAND: 'app'}, targets=targets)))
                packed = run('pack', str(root))
                archive = Path(packed['archive']).read_bytes()
                archives[channel, version] = archive
                releases[channel].insert(0, dict(app_id=APP, channel=channel, version=version,
                    sha256=hashlib.sha256(archive).hexdigest(), size=len(archive), created_at=0))

        class Handler(http.server.BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass

            def do_GET(self):
                parsed = urllib.parse.urlsplit(self.path)
                query = urllib.parse.parse_qs(parsed.query)
                channel = query.get('channel', ['prod'])[0]
                requests.append((time.time(), parsed.path, channel))
                if parsed.path.endswith('/releases'):
                    body = json.dumps({'items': releases[channel]}).encode()
                elif parsed.path.endswith('/download'):
                    body = archives[channel, query['version'][0]]
                else:
                    self.send_error(404)
                    return
                self.send_response(200)
                self.send_header('Content-Length', str(len(body)))
                self.end_headers()
                self.wfile.write(body)

        with http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler) as server:
            threading.Thread(target=server.serve_forever, daemon=True).start()
            env['HONEYCOMB_API_URL'] = f'http://127.0.0.1:{server.server_port}'
            installed = run('install', APP + '@1.0.0')
            assert installed['version'] == '1.0.0' and installed['channel'] == 'prod'
            registry = next((home / '.honeycomb').rglob('installed.json'))
            before = registry.read_bytes()
            error = run('install', APP + '>test@1.0.0', ok=False)
            assert '--switch-channel' in error.stderr and registry.read_bytes() == before
            installed = run('install', APP + '>test@1.0.0', '--switch-channel')
            assert installed['channel'] == 'dev'
            assert list(json.loads(registry.read_text())) == [APP + '>test']
            # Verify the real interactive confirmation, including decline preserving state.
            master, slave = pty.openpty()
            child = subprocess.Popen([str(CLI), 'install', APP + '@1.0.0'], env=env,
                                     stdin=slave, stderr=slave, stdout=subprocess.PIPE)
            os.close(slave)
            text = b''
            deadline = time.time() + 10
            while b'[y/N]' not in text and time.time() < deadline:
                if select.select([master], [], [], .2)[0]:
                    text += os.read(master, 8192)
            assert b'official production releases' in text, text
            os.write(master, b'n\n')
            child.communicate(timeout=10)
            os.close(master)
            assert child.returncode != 0
            assert list(json.loads(registry.read_text())) == [APP + '>test']
            run('install', APP + '@1.0.0', '--switch-channel')
            run('install', APP + '>test@1.0.0', '--alias', COMMAND + '=' + COMMAND + '-dev')
            records = json.loads(registry.read_text())
            assert set(records) == {APP, APP + '>test'}
            assert records[APP]['directory'] != records[APP + '>test']['directory']
            run('update', APP + '>test')
            records = json.loads(registry.read_text())
            assert records[APP]['version'] == '1.0.0'
            assert records[APP + '>test']['version'] == '2.0.0'
            assert records[APP + '>test']['aliases'][COMMAND] == COMMAND + '-dev'
            # Move the dev install back explicitly so one daemon cycle must advance both tracks.
            run('install', APP + '>test@1.0.0')
            config_path = home / '.honeycomb/dir/config.json'
            config = json.loads(config_path.read_text()) if config_path.exists() else {}
            # Avoid a real CLI release network request while exercising app maintenance.
            config.update(auto_update=True, last_update_check=int(time.time()) + 3600)
            config_path.write_text(json.dumps(config))
            # An ordinary app install registers its worker without requiring `service install`.
            # Stub only the OS supervisor, leaving CLI registration and files real and isolated.
            supervisor_bin = home / 'supervisor-bin'
            supervisor_bin.mkdir()
            supervisor = supervisor_bin / ('launchctl' if sys.platform == 'darwin' else 'systemctl')
            supervisor.write_text('#!/bin/sh\nprintf "%s\\n" "$*" >> "$HOME/supervisor-calls"\nif [ "$1" = print ]; then exit 1; fi\nexit 0\n')
            supervisor.chmod(0o755)
            service_env = {k: v for k, v in env.items() if k not in ('HONEYCOMB_AUTO_UPDATE', 'HONEYCOMB_NO_SERVICE')}
            service_env['PATH'] = str(supervisor_bin) + os.pathsep + env['PATH']
            setup = run('install', APP + '@1.0.0', environment=service_env)
            assert setup['auto_update']['status'] == 'enabled', setup
            calls = (home / 'supervisor-calls').read_text()
            assert ('bootstrap' if sys.platform == 'darwin' else 'enable --now') in calls, calls
            (registry.parent / 'last-package-update.json').write_text('0')
            daemon_env = {k: v for k, v in env.items() if k != 'HONEYCOMB_AUTO_UPDATE'}
            before_requests = len(requests)
            daemon = subprocess.Popen([str(CLI), 'daemon'], env=daemon_env,
                                      stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            try:
                # The daemon waits for second 01, without user commands triggering checks.
                deadline = time.time() + 65
                while time.time() < deadline:
                    assert daemon.poll() is None, daemon.communicate()
                    records = json.loads(registry.read_text())
                    if all(r['version'] == '2.0.0' for r in records.values()):
                        break
                    time.sleep(.1)
                else:
                    raise AssertionError('Minute daemon did not update both installed tracks')
                fetched = requests[before_requests:]
                assert {r[2] for r in fetched} == {'prod', 'dev'}, fetched
                first_time = fetched[0][0] % 60
                assert 1 <= first_time < 5, fetched
            finally:
                daemon.terminate()
                daemon.communicate(timeout=10)
            for key, expected in [(APP, 'prod-2.0.0'), (APP + '>test', 'dev-2.0.0')]:
                executable = records[key]['commands'][COMMAND]
                assert subprocess.check_output([executable], text=True).strip() == expected
            run('uninstall', APP + '>test')
            assert set(json.loads(registry.read_text())) == {APP}
            assert subprocess.check_output([records[APP]['commands'][COMMAND]], text=True).strip() == 'prod-2.0.0'
            server.shutdown()
    print('PASS: exact versions, channel prompts/decline/confirmation, coexistence, alias preservation, channel-specific updates, minute daemon at second 01, independent uninstall')


if __name__ == '__main__':
    main()
