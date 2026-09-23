#!/usr/bin/env python3
"""Privileged, read-only-IAM import of a pre-Honeycomb application into its backend.

Run on the backend host after deploying revision-zero reconciliation support.
Defaults to a preview. --apply requires a consistent SQLite backup and records
an operator audit. Never creates, reconfigures, publishes, or rotates an IAM app.
"""
import argparse
import copy
import hashlib
import json
import os
import re
from pathlib import Path
import sqlite3
import time
import urllib.parse
import urllib.request
import uuid


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        raise RuntimeError("IAM redirected the authenticated request")


def read_record(settings, app):
    base = settings.get("IAM_BASE_URL", "https://backend.iam.teamofsilicons.com").rstrip("/")
    if urllib.parse.urlparse(base).scheme != "https":
        raise ValueError("IAM must use HTTPS")
    request = urllib.request.Request(
        base + "/api/v1/honeycomb/applications/" + urllib.parse.quote(app, safe=""),
        headers={"Authorization": "Bearer " + settings["IAM_HONEYCOMB_SERVICE_CREDENTIAL"]},
    )
    with urllib.request.build_opener(NoRedirect).open(request, timeout=30) as response:
        return json.load(response)


def preserved_visibility(record, preserve_public):
    if not preserve_public:
        return "private"
    if (record.get("visibility") != "public" or record.get("availability") != "verified"
            or type(record.get("configuration_revision")) is not int or record["configuration_revision"] != 0
            or type(record.get("iam_revision")) is not int or record["iam_revision"] < 1
            or type(record.get("credential_version")) is not int or record["credential_version"] < 1):
        raise ValueError("Preserving public visibility requires an existing verified public IAM identity at configuration revision zero")
    return "public"


def projection(record, app, expected_identity, metadata, preserve_public=False):
    org = metadata["org_id"]
    if not re.fullmatch(r"[a-z][a-z0-9_-]{0,79}", app) or not re.fullmatch(r"[a-z0-9_-]{3,50}", org):
        raise ValueError("Supply a bare app ID and explicit metadata org_id")
    if record.get("app_id") != app or record.get("org_id") != org:
        raise ValueError("IAM identity or owning organization mismatch")
    if record.get("application_id") != expected_identity:
        raise ValueError("IAM immutable application identity mismatch")
    if type(record.get("configuration_revision")) is not int or record["configuration_revision"] != 0 or type(record.get("iam_revision")) is not int or record["iam_revision"] <= 0:
        raise ValueError("Only established legacy IAM applications at configuration revision zero can be adopted")
    if record.get("availability") != "verified" or type(record.get("credential_version")) is not int or record["credential_version"] < 1:
        raise ValueError("IAM application must be verified with existing credentials")
    visibility = preserved_visibility(record, preserve_public)
    description = metadata["description"].strip()
    if not 50 <= len(description.split()) <= 1000:
        raise ValueError("Catalog description must contain 50–1000 words")
    granted = {item["scope"] for item in record["effective_scopes"]}
    config = {
        "org_id": org, "app_id": app, "name": record["app_name"],
        "description": description,
        "app_scope": {
            "iam": [scope for scope in record["app_scope"]["iam"] if scope in granted],
            "external": [scope for scope in record["app_scope"]["external"]
                         if "obo:" + scope["app_id"] + ":" + scope["endpoint_id"] in granted],
        },
    }
    for dest, source in [("logo_url", "app_logo"), ("base_url", "base_url"),
                         ("obo_endpoints", "obo_endpoints"), ("obo_review_message", "obo_review_message"),
                         ("testing_idle_days", "testing_idle_days"), ("webhook_scope", "webhook_scope")]:
        config[dest] = record[source]
    for field in ("website_url", "docs_url"):
        if metadata.get(field):
            if urllib.parse.urlparse(metadata[field]).scheme != "https":
                raise ValueError("Catalog links must use HTTPS")
            config[field] = metadata[field]
    if preserve_public:
        # Preserve declarations, including pending requests, without granting them.
        # insert() still limits the effective projection to IAM's granted scopes.
        config["app_scope"] = copy.deepcopy(record["app_scope"])
        config["visibility"] = visibility
    # IAM intentionally does not disclose accepted webhook URL or any secrets.
    return config


def insert(db, record, config, actor, preserve_public=False):
    visibility = preserved_visibility(record, preserve_public)
    effective = copy.deepcopy(config)
    granted = {item["scope"] for item in record["effective_scopes"]}
    effective["app_scope"]["iam"] = [scope for scope in effective["app_scope"]["iam"] if scope in granted]
    effective["app_scope"]["external"] = [scope for scope in effective["app_scope"]["external"]
        if "obo:" + scope["app_id"] + ":" + scope["endpoint_id"] in granted]
    now = int(time.time())
    app = record["app_id"]
    op = str(uuid.uuid4())
    serialized = json.dumps(config, sort_keys=True)
    receipt = {key: record[key] for key in ("app_id", "application_id", "iam_revision", "configuration_revision", "credential_version")}
    receipt.update({"operation_id": op, "visibility": visibility, "state": "active"})
    if preserve_public:
        receipt["preserved_public_visibility"] = True
    # Plain INSERT deliberately rejects duplicates; never overwrite any local app.
    db.execute("""INSERT INTO applications(plane,app_id,org_id,name,description,visibility,state,
        revision,iam_revision,config,effective_config,webhook_secret,created_at,updated_at,
        effective_revision,credential_version,iam_check_after)
        VALUES('production',?,?,?,?,?,'active',0,?,?,?,'',?,?,0,?,0)""",
        (app, config["org_id"], config["name"], config["description"], visibility, record["iam_revision"],
         serialized, json.dumps(effective, sort_keys=True), now, now, record["credential_version"]))
    db.execute("""INSERT INTO operations(id,plane,actor,idempotency_key,kind,resource,request_hash,revision,state,result,created_at)
        VALUES(?,'production',?,?,'iam.adopt',?,?,0,'accepted',?,?)""",
        (op, actor, op, app, hashlib.sha256(serialized.encode()).hexdigest(), json.dumps(receipt), now))
    db.execute("INSERT INTO audit(id,plane,actor,action,resource,created_at) VALUES(?,'production',?,'iam.adopt',?,?)",
               (str(uuid.uuid4()), actor, app, now))
    return receipt


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--app", required=True)
    p.add_argument("--expected-identity", required=True)
    p.add_argument("--metadata", required=True, type=Path)
    p.add_argument("--database", required=True, type=Path)
    p.add_argument("--env-file", required=True, type=Path)
    p.add_argument("--actor", required=True, help="Auditable operator identity/reason")
    p.add_argument("--backup-dir", type=Path)
    p.add_argument("--apply", action="store_true")
    p.add_argument("--preserve-public-visibility", action="store_true",
                   help="Preserve an IAM-confirmed existing public identity during catalog adoption; never publishes a private identity")
    args = p.parse_args()
    settings = dict(line.split("=", 1) for line in args.env_file.read_text().splitlines()
                    if line.strip() and not line.lstrip().startswith("#"))
    record = read_record(settings, args.app)
    config = projection(record, args.app, args.expected_identity, json.loads(args.metadata.read_text()), args.preserve_public_visibility)
    db = sqlite3.connect(args.database.resolve().as_uri() + "?mode=rw", uri=True, timeout=30)
    db.execute("PRAGMA foreign_keys=ON")
    if db.execute("SELECT 1 FROM applications WHERE plane='production' AND app_id=?", (args.app,)).fetchone():
        raise ValueError("Application already exists in Honeycomb; refusing to overwrite")
    if not args.apply:
        print(json.dumps({"preview": True, "app_id": args.app, "iam_revision": record["iam_revision"], "visibility": preserved_visibility(record, args.preserve_public_visibility), "preserved_public_visibility": args.preserve_public_visibility, "configuration": config}, indent=2))
        return
    if not args.backup_dir:
        p.error("--apply requires --backup-dir")
    os.umask(0o077)
    args.backup_dir.mkdir(parents=True, exist_ok=True)
    backup = args.backup_dir / (time.strftime("%Y%m%dT%H%M%SZ", time.gmtime()) + "-" + str(uuid.uuid4()) + ".db")
    with sqlite3.connect(backup) as destination:
        db.backup(destination)
        if destination.execute("PRAGMA integrity_check").fetchone()[0] != "ok":
            raise RuntimeError("Backup integrity check failed")
    db.execute("BEGIN IMMEDIATE")
    try:
        if read_record(settings, args.app) != record:
            raise RuntimeError("IAM changed during import; retry from a fresh preview")
        receipt = insert(db, record, config, args.actor, args.preserve_public_visibility)
        db.commit()
    except BaseException:
        db.rollback()
        raise
    print(json.dumps({"receipt": receipt, "backup": str(backup)}, indent=2))


if __name__ == "__main__":
    main()
