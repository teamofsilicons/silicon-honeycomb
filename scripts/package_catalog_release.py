#!/usr/bin/env python3
"""Build Honeycomb's own catalog package from six verified native release assets."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tarfile
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--assets', required=True, type=Path)
    parser.add_argument('--version', required=True)
    parser.add_argument('--cli', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    if not re.fullmatch(r'(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)', args.version):
        parser.error('--version must be x.y.z')
    if args.output.exists():
        parser.error('--output already exists; retain immutable release artifacts')
    targets = {}
    with tempfile.TemporaryDirectory(prefix='honeycomb-catalog-') as temporary:
        root = Path(temporary)
        for system in ['linux', 'macos', 'windows']:
            for architecture in ['x86_64', 'aarch64']:
                target = f'{system}-{architecture}'
                asset = args.assets / f'honeycomb-{target}.tar.gz'
                checksum = asset.with_name(asset.name + '.sha256').read_text().split()
                if len(checksum) != 2 or checksum[1] != asset.name:
                    raise ValueError(f'Invalid checksum file for {target}')
                actual = hashlib.sha256(asset.read_bytes()).hexdigest()
                if actual != checksum[0]:
                    raise ValueError(f'Checksum mismatch for {target}')
                binary = 'honeycomb.exe' if system == 'windows' else 'honeycomb'
                destination = root / 'targets' / target / 'bin' / binary
                destination.parent.mkdir(parents=True)
                with tarfile.open(asset, 'r:gz') as archive:
                    entries = archive.getmembers()
                    if len(entries) != 1 or entries[0].name != binary or not entries[0].isfile():
                        raise ValueError(f'Unexpected native archive contents for {target}')
                    with archive.extractfile(entries[0]) as source, destination.open('wb') as output:
                        shutil.copyfileobj(source, output)
                destination.chmod(0o755)
                targets[target] = {'root': f'targets/{target}', 'executables': {'app': f'bin/{binary}'}}
        manifest = {'format_version': 1, 'app_id': 'honeycomb', 'version': args.version,
                    'bin': {'honeycomb': 'app'}, 'targets': targets}
        (root / 'honeycomb.yaml').write_text(json.dumps(manifest, indent=2) + '\n')
        shutil.copyfile(Path(__file__).resolve().parents[1] / 'LICENSE', root / 'LICENSE')
        env = dict(os.environ, HONEYCOMB_AUTO_UPDATE='0', HONEYCOMB_NO_SERVICE='1', HONEYCOMB_TELEMETRY='0')
        subprocess.run([str(args.cli.resolve()), 'pack', str(root), '--output', str(args.output.resolve()), '--json'],
                       env=env, check=True)


if __name__ == '__main__':
    main()
