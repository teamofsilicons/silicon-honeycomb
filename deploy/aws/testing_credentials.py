"""Provision Honeycomb/Briefcase testing credentials during managed deployment.

This credential is deployment plumbing, never an app-registration or user setup step.
Existing credentials are preserved; a partially completed update is safe to resume.
"""
import copy
import hmac
import json
import os
from pathlib import Path
import secrets
import subprocess
import tempfile

TOKEN = "BRIEFCASE_HONEYCOMB_SERVICE_TOKEN"


def desired_credentials(honeycomb, briefcase):
    """Return updated copies without rotating an existing service credential."""
    honeycomb = copy.deepcopy(honeycomb)
    briefcase = copy.deepcopy(briefcase)
    backend = honeycomb["backend"]
    tokens = [value for value in (backend.get(TOKEN), briefcase.get(TOKEN)) if value]
    for token in tokens:
        if not isinstance(token, str) or len(token) < 32 or not all(33 <= ord(c) <= 126 for c in token):
            raise ValueError("Existing testing service credential has invalid format")
    if len(tokens) == 2 and not hmac.compare_digest(*tokens):
        raise ValueError("Testing service credentials differ; refusing to overwrite either service")
    token = tokens[0] if tokens else secrets.token_urlsafe(48)
    backend[TOKEN] = briefcase[TOKEN] = token
    backend.setdefault("BRIEFCASE_BASE_URL", "https://backend.briefcase.teamofsilicons.com")
    briefcase.setdefault("BRIEFCASE_HONEYCOMB_BASE_URL", "https://backend.honeycomb.teamofsilicons.com/")
    return honeycomb, briefcase


def ensure_testing_credentials(profile, honeycomb_region, honeycomb_secret,
                               briefcase_region="us-east-1",
                               briefcase_secret="silicon-briefcase/production"):
    def aws(region, *command):
        result = subprocess.run(
            ["aws", "--profile", profile, "--region", region, "secretsmanager",
             *command, "--output", "json"], capture_output=True, text=True,
        )
        if result.returncode:
            # AWS error text can contain request content; never echo it with secrets.
            raise RuntimeError("Testing credential secret-store operation failed")
        return json.loads(result.stdout)

    def read(region, name):
        result = aws(region, "get-secret-value", "--secret-id", name)
        return result["VersionId"], json.loads(result["SecretString"])

    def write_if_changed(region, name, version, previous, desired):
        if previous == desired:
            return False
        latest_version, _ = read(region, name)
        if latest_version != version:
            raise RuntimeError("Service secret changed during deployment; retry with current configuration")
        fd, filename = tempfile.mkstemp(prefix="honeycomb-testing-credential-")
        try:
            os.fchmod(fd, 0o600)
            with os.fdopen(fd, "w") as stream:
                json.dump(desired, stream)
            aws(region, "put-secret-value", "--secret-id", name,
                "--secret-string", "file://" + filename)
        finally:
            Path(filename).unlink(missing_ok=True)
        return True

    hc_version, hc = read(honeycomb_region, honeycomb_secret)
    bc_version, bc = read(briefcase_region, briefcase_secret)
    next_hc, next_bc = desired_credentials(hc, bc)
    # Briefcase first: if the second write fails, retry reuses its credential.
    changed_bc = write_if_changed(briefcase_region, briefcase_secret, bc_version, bc, next_bc)
    changed_hc = write_if_changed(honeycomb_region, honeycomb_secret, hc_version, hc, next_hc)
    _, verified_hc = read(honeycomb_region, honeycomb_secret)
    _, verified_bc = read(briefcase_region, briefcase_secret)
    if not hmac.compare_digest(verified_hc["backend"].get(TOKEN, ""), next_hc["backend"][TOKEN]) or not hmac.compare_digest(verified_bc.get(TOKEN, ""), next_bc[TOKEN]):
        raise RuntimeError("Testing credential changed during verification; deployment stopped")
    return {"honeycomb_secret_updated": changed_hc, "briefcase_secret_updated": changed_bc}
