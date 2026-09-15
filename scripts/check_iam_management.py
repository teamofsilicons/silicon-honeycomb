#!/usr/bin/env python3
"""Run the read-only SDK probe with a protected dotenv handoff, without shell evaluation."""
import argparse
import os
from pathlib import Path
import shlex
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("handoff", type=Path)
    parser.add_argument("--require-ready", action="store_true", help="Fail if required effective scopes are missing")
    args = parser.parse_args()
    env = os.environ.copy()
    allowed = {"IAM_BASE_URL", "HONEYCOMB_APP_ID", "IAM_HONEYCOMB_SERVICE_CREDENTIAL"}
    for line in args.handoff.read_text().splitlines():
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        key, separator, value = line.partition("=")
        if separator and key.strip() in allowed:
            parts = shlex.split(value, comments=True)
            if len(parts) != 1:
                parser.error(f"Expected one value for {key.strip()}")
            env[key.strip()] = parts[0]
    missing = allowed - env.keys()
    if missing:
        parser.error(f"Missing settings: {', '.join(sorted(missing))}")
    return subprocess.call(
        ["cargo", "run", "-p", "silicon-honeycomb-server", "--example", "iam_management_check", "--locked"]
        + (["--", "--require-ready"] if args.require_ready else []),
        cwd=Path(__file__).resolve().parents[1], env=env,
    )


if __name__ == "__main__":
    raise SystemExit(main())
