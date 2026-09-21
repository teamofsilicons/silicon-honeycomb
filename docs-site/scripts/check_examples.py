"""Exercise the downloadable archive layout using the real CLI in a disposable home."""
import json,os,shutil,subprocess,tempfile
from pathlib import Path
repo=Path(__file__).resolve().parents[2]
binary=repo/'target/debug/honeycomb'
with tempfile.TemporaryDirectory(prefix='honeycomb-docs-') as tmp:
 root=Path(tmp); package=root/'package';package.mkdir()
 shutil.copyfile(repo/'docs-site/public/examples/honeycomb.yaml',package/'honeycomb.yaml')
 for target in ['linux-x86_64','linux-aarch64','windows-x86_64','windows-aarch64','macos-x86_64','macos-aarch64']:
  folder=package/'targets'/target;folder.mkdir(parents=True)
  exe=folder/('my-app.exe' if target.startswith('windows') else 'my-app')
  exe.write_bytes(b'MZ docs structure fixture' if target.startswith('windows') else b'#!/bin/sh\nexit 0\n');exe.chmod(0o755)
 env=dict(os.environ,SILICON_HOME=str(root/'home'),HONEYCOMB_TELEMETRY='false',HONEYCOMB_AUTO_UPDATE='0',HONEYCOMB_NO_SERVICE='1')
 for command in [['validate',str(package)],['pack',str(package),'--output',str(root/'release.tar.gz')],['validate',str(root/'release.tar.gz')]]:
  subprocess.run([str(binary),*command],env=env,check=True,capture_output=True,text=True)
 print('PASS: downloadable manifest validates, packs, and revalidates with six fixture payloads (structural check, not native binary execution).')
