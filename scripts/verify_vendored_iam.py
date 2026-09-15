#!/usr/bin/env python3
"""Verify the unchanged official SDK sources against the pinned upstream manifest."""
import hashlib
import json
from pathlib import Path

root = Path(__file__).resolve().parents[1] / 'vendor/silicon-iam-client'
manifest = json.loads((root / 'UPSTREAM.json').read_text())
expected = manifest['source_sha256']
actual = {str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest()
          for p in (root / 'src').rglob('*') if p.is_file()}
assert actual == expected, 'Vendored IAM source differs from its pinned official snapshot'
print(f"PASS: official IAM {manifest['version']} source at {manifest['commit']}")
