#!/usr/bin/env python3
"""Validate Cargo installation without publishing or replacing a user's CLI."""
import json
import os
from pathlib import Path
import subprocess
import tempfile

repo = Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix='honeycomb-cargo-install-') as temp:
    root = Path(temp)
    env = dict(os.environ, SILICON_HOME=str(root), CARGO_TARGET_DIR=str(repo / 'target'))
    subprocess.run(['cargo', 'install', '--locked', '--debug', '--path', str(repo / 'crates/cli'), '--root', str(root / 'cargo'), '--bin', 'honeycomb'], env=env, check=True)
    binary = root / 'cargo/bin/honeycomb'
    assert subprocess.check_output([str(binary), '--version'], env=env, text=True).startswith('honeycomb ')
    subprocess.run([str(binary), 'config', 'set', 'auto_update', 'false'], env=env, check=True, stdout=subprocess.DEVNULL)
    subprocess.run([str(binary), 'daemon', '--once'], env=env, check=True, timeout=10)
    result = subprocess.check_output([str(binary), 'login', 'status', '--json'], env=env, text=True)
    assert json.loads(result) == {'authenticated': False}
    print('PASS: isolated Cargo source installation, executable/version, unauthenticated status, scheduled updater opt-out')
