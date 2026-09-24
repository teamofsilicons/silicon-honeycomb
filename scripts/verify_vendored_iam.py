#!/usr/bin/env python3
"""Verify the SDK sources against their recorded upstream snapshot."""
import hashlib
import json
from pathlib import Path

root = Path(__file__).resolve().parents[1] / 'vendor/silicon-iam-client'
manifest = json.loads((root / 'UPSTREAM.json').read_text())
expected = manifest['source_sha256']
actual = {str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest()
          for p in (root / 'src').rglob('*') if p.is_file()}
assert actual == expected, 'Vendored IAM source differs from its recorded upstream snapshot'
kind = "working-tree snapshot based on" if manifest.get("dirty") else "source at"
print(f"PASS: IAM {manifest['version']} {kind} {manifest['commit']}")
