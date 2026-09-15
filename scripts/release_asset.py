#!/usr/bin/env python3
"""Package and smoke-test the native CLI on each release runner."""
import hashlib
from pathlib import Path
import platform
import subprocess
import sys
import tarfile

os_name = {'Darwin': 'macos', 'Linux': 'linux', 'Windows': 'windows'}[platform.system()]
architecture = {'x86_64': 'x86_64', 'amd64': 'x86_64', 'arm64': 'aarch64', 'aarch64': 'aarch64'}[platform.machine().lower()]
target = sys.argv[1]
assert target == f'{os_name}-{architecture}', f'Release target does not match this runner: {target}'
name = 'honeycomb.exe' if os_name == 'windows' else 'honeycomb'
binary = Path('target/release') / name
version = subprocess.check_output([str(binary.resolve()), '--version'], text=True).strip()
assert version.startswith('honeycomb ')
help_text = subprocess.check_output([str(binary.resolve()), '--help'], text=True)
assert 'install' in help_text and 'login' in help_text
license_text = subprocess.check_output([str(binary.resolve()), 'license'], text=True)
assert license_text == Path('LICENSE').read_text(), 'Release binary must contain the MIT notice'
out = Path('release-assets')
out.mkdir(exist_ok=True)
archive = out / f'honeycomb-{target}.tar.gz'
with tarfile.open(archive, 'w:gz') as tar:
    tar.add(binary, arcname=name)
archive.with_name(archive.name + '.sha256').write_text(hashlib.sha256(archive.read_bytes()).hexdigest() + '  ' + archive.name + '\n')
print(f'{version}: verified and packaged {archive}')
