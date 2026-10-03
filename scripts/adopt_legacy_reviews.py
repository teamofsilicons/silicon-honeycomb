#!/usr/bin/env python3
"""Audited, two-phase handover of pending IAM reviews into Honeycomb.

Prepare imports an absent catalog entry as private, with its original discussions. Then
an authorized app manager calls Honeycomb's ordinary review-plan endpoint. Finalize
verifies that real IAM plan before enabling old email links. Neither phase approves,
publishes, rotates a credential, calls an IAM mutation, or sends mail.

The source is a protected operator export, pinned by an explicit SHA-256. Runtime
IAM credentials and Honeycomb's encryption key are read only from the host env file.
Prepare requires the Python cryptography package for AES-256-GCM at-rest encryption.
"""
import argparse
import copy
from datetime import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import sqlite3
import time
import urllib.parse
import urllib.request
import uuid

from adopt_legacy_app import NoRedirect, read_record


def encoded(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def canonical_uuid(value):
    if str(uuid.UUID(value)) != value:
        raise ValueError("Request and message identities must be canonical UUIDs")
    return value


def timestamp(value):
    if isinstance(value, str):
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
        if parsed.tzinfo is None or parsed.timestamp() < 0:
            raise ValueError("Source timestamps require an explicit timezone")
        # Honeycomb display timestamps use seconds. The untouched source string,
        # including subsecond precision, remains in the handover provenance.
        return int(parsed.timestamp())
    if type(value) is not int or value < 0:
        raise ValueError("Source timestamps must be timezone-aware ISO dates or Unix seconds")
    return value


def principal(value):
    if not isinstance(value, str) or not re.fullmatch(r"(?:c|si):[a-zA-Z0-9_-]+", value):
        raise ValueError("Preserve a canonical source principal")
    return value


def validate(source):
    if set(source) != {"schema", "application", "configuration", "webhook_secret", "requests"}:
        raise ValueError("Unexpected source fields; use the versioned handover export")
    if source["schema"] != "honeycomb.legacy-review-handover.v1":
        raise ValueError("Unsupported handover schema")
    record, config, requests = source["application"], source["configuration"], source["requests"]
    if (record.get("visibility"), record.get("availability")) not in {
            ("private", "verified"), ("public", "under_review")}:
        raise ValueError("Only private verified or public still-under-review legacy identities can move")
    if not re.fullmatch(r"[a-z][a-z0-9_-]{0,79}", record["app_id"]) or not re.fullmatch(r"[a-z0-9_-]{3,50}", record["org_id"]):
        raise ValueError("Invalid legacy application identity")
    if type(record.get("configuration_revision")) is not int or record["configuration_revision"] != 0 or type(record.get("iam_revision")) is not int or record["iam_revision"] <= 0:
        raise ValueError("Only an established revision-zero IAM identity can move")
    if not isinstance(record.get("application_id"), str) or not record["application_id"]:
        raise ValueError("The immutable IAM application identity is required")
    credential = record.get("credential_version")
    if credential is not None and (type(credential) is not int or credential < 1):
        raise ValueError("Invalid current credential version")
    allowed = {"org_id", "app_id", "name", "description", "logo_url", "base_url",
               "website_url", "docs_url", "app_scope", "webhook_url", "webhook_scope",
               "obo_endpoints", "ata_endpoints", "obo_review_message", "testing_idle_days", "visibility"}
    if not isinstance(config, dict) or not set(config) <= allowed:
        raise ValueError("Configuration contains unexpected or secret fields")
    if not 50 <= len(config.get("description", "").split()) <= 1000:
        raise ValueError("Catalog description must contain 50–1000 words")
    for field, source_field in (("app_id", "app_id"), ("org_id", "org_id"), ("name", "app_name"),
            ("logo_url", "app_logo"), ("base_url", "base_url"), ("obo_endpoints", "obo_endpoints"),
            ("obo_review_message", "obo_review_message"), ("testing_idle_days", "testing_idle_days"), ("webhook_scope", "webhook_scope")):
        if config.get(field) != record.get(source_field):
            raise ValueError("Source configuration differs from the IAM identity: " + field)
    for field in ("website_url", "docs_url"):
        if config.get(field) and urllib.parse.urlparse(config[field]).scheme != "https":
            raise ValueError("Catalog links must use HTTPS")
    if config.get("app_scope") != record["app_scope"] or config.get("ata_endpoints", []) != record.get("ata_endpoints", []):
        raise ValueError("Handover must preserve exact declared scopes and endpoints")
    if config.get("visibility", "public") != "public":
        raise ValueError("Imported request is a pending public review, while the app remains private")
    url = urllib.parse.urlparse(config.get("webhook_url", ""))
    if url.scheme != "https" or not url.netloc or url.username or url.password or url.fragment:
        raise ValueError("Supply the existing HTTPS webhook destination")
    if not isinstance(source["webhook_secret"], str) or not source["webhook_secret"]:
        raise ValueError("Supply the existing webhook secret through the protected export")
    if not isinstance(requests, list) or not requests:
        raise ValueError("At least one pending legacy review is required")
    request_ids, message_ids, providers, authors = set(), set(), set(), set()
    declared = set(record["app_scope"]["iam"]) | {
        "obo:" + v["app_id"] + ":" + v["endpoint_id"] for v in record["app_scope"]["external"]}
    for request in requests:
        if set(request) != {"id", "app_id", "provider", "scopes", "status", "requested_by", "created_at", "updated_at", "messages"}:
            raise ValueError("Incomplete or unexpected original request fields")
        request_id = canonical_uuid(request["id"])
        provider = request["provider"]
        if request_id in request_ids or provider in providers or provider == "honeycomb" or not re.fullmatch(r"[a-z][a-z0-9_-]{0,79}", provider):
            raise ValueError("Duplicate or unsupported legacy review identity/provider")
        request_ids.add(request_id)
        providers.add(provider)
        authors.add(principal(request["requested_by"]))
        if request["app_id"] != record["app_id"] or request["status"] != "pending":
            raise ValueError("Only this application's still-pending reviews can move")
        scopes = request["scopes"]
        if not isinstance(scopes, list) or not scopes or len(set(scopes)) != len(scopes) or not set(scopes) <= declared:
            raise ValueError("Original scopes must be unique and still declared")
        if any((s.startswith("obo:") if provider == "iam" else not s.startswith("obo:" + provider + ":")) for s in scopes):
            raise ValueError("Original scopes do not belong to this review provider")
        if timestamp(request["updated_at"]) < timestamp(request["created_at"]):
            raise ValueError("Original request timestamps are inconsistent")
        if not isinstance(request["messages"], list) or not request["messages"]:
            raise ValueError("Preserve the original request justification")
        for message in request["messages"]:
            if set(message) != {"id", "actor", "message", "created_at"}:
                raise ValueError("Incomplete or unexpected original message fields")
            message_id = canonical_uuid(message["id"])
            if message_id in message_ids:
                raise ValueError("Duplicate source message identity")
            message_ids.add(message_id)
            if message["actor"] is not None:
                principal(message["actor"])
            if not isinstance(message["message"], str) or not 1 <= len(message["message"]) <= 10000:
                raise ValueError("Invalid original discussion message")
            if not timestamp(request["created_at"]) <= timestamp(message["created_at"]) <= timestamp(request["updated_at"]):
                raise ValueError("Original message is outside its request timestamps")
    if len(authors) != 1:
        raise ValueError("This handover requires one original requesting principal")
    return copy.deepcopy(config)


def prepare(db, source, source_hash, actor, encrypt):
    config = validate(source)
    record = source["application"]
    existing = db.execute("SELECT request_id,state FROM legacy_review_handovers WHERE source_sha256=?", (source_hash,)).fetchone()
    if existing:
        return {"request_id": existing[0], "state": existing[1], "replayed": True}
    if db.execute("SELECT 1 FROM applications WHERE plane='production' AND app_id=?", (record["app_id"],)).fetchone():
        raise ValueError("Application already exists; refusing to overwrite or merge unrelated work")
    config["visibility"] = "public"
    now = int(time.time())
    created = min(timestamp(r["created_at"]) for r in source["requests"])
    requested_by = source["requests"][0]["requested_by"]
    request_id = str(uuid.uuid5(uuid.NAMESPACE_URL, "honeycomb:legacy-review:" + source_hash))
    effective = copy.deepcopy(config)
    granted = {item["scope"] for item in record["effective_scopes"]}
    effective["app_scope"]["iam"] = [scope for scope in effective["app_scope"]["iam"] if scope in granted]
    effective["app_scope"]["external"] = [scope for scope in effective["app_scope"]["external"] if "obo:" + scope["app_id"] + ":" + scope["endpoint_id"] in granted]
    effective["visibility"] = "private"
    effective.pop("webhook_url", None)
    state = "active" if record["availability"] == "verified" else "disabled"
    op = str(uuid.uuid4())
    receipt = {key: record[key] for key in ("app_id", "application_id", "iam_revision", "configuration_revision", "credential_version")}
    receipt.update(operation_id=op, visibility="private", state=state, review_handover=True)
    db.execute("""INSERT INTO applications(plane,app_id,org_id,name,description,visibility,state,revision,iam_revision,
        config,effective_config,webhook_secret,created_at,updated_at,effective_revision,credential_version,iam_check_after)
        VALUES('production',?,?,?,?,'private',?,1,?,?,?,?,?,?,0,?,0)""",
        (record["app_id"], config["org_id"], config["name"], config["description"], state, record["iam_revision"],
         encoded(config), encoded(effective), encrypt(source["webhook_secret"]), now, now, record["credential_version"] or 0))
    db.execute("""INSERT INTO operations(id,plane,actor,idempotency_key,kind,resource,request_hash,revision,state,result,created_at)
        VALUES(?,'production',?,?,'iam.adopt',?,?,0,'accepted',?,?)""",
        (op, actor, "legacy-review:" + source_hash, record["app_id"], source_hash, encoded(receipt), now))
    db.execute("""INSERT INTO publication_requests(id,plane,app_id,revision,state,requested_by,created_at,config_snapshot)
        VALUES(?,'production',?,1,'awaiting_review_plan',?,?,?)""",
               (request_id, record["app_id"], requested_by, created, encoded(config)))
    for request in source["requests"]:
        for message in request["messages"]:
            db.execute("INSERT INTO discussions(id,request_id,actor,provider,message,created_at) VALUES(?,?,?,?,?,?)",
                       (message["id"], request_id, message["actor"] or "legacy-iam-system", request["provider"], message["message"], timestamp(message["created_at"])))
    # Make the newly available imported request discoverable without changing its
    # original creation/message timestamps or falsely marking it read.
    db.execute("UPDATE publication_activity SET updated_at=? WHERE request_id=?", (now, request_id))
    db.execute("""INSERT INTO legacy_review_handovers(source_sha256,plane,app_id,application_id,request_id,state,prepared_at,source_requests)
        VALUES(?,'production',?,?,?,'prepared',?,?)""",
               (source_hash, record["app_id"], record["application_id"], request_id, now, encoded(source["requests"])))
    db.execute("INSERT INTO audit(id,plane,actor,action,resource,created_at) VALUES(?,'production',?,'iam.review_handover.prepare',?,?)",
               (str(uuid.uuid4()), actor, record["app_id"], now))
    return {"request_id": request_id, "app_id": record["app_id"], "state": "prepared", "adoption_operation_id": receipt["operation_id"]}


def finalize(db, source, source_hash, actor, plan):
    config = validate(source)
    config["visibility"] = "public"
    record = source["application"]
    handover = db.execute("SELECT request_id,state,source_requests FROM legacy_review_handovers WHERE source_sha256=?", (source_hash,)).fetchone()
    if not handover or json.loads(handover[2]) != source["requests"]:
        raise ValueError("Prepare this exact source before finalizing")
    request_id = handover[0]
    if plan.get("state") != "accepted" or plan.get("request_id") != request_id or plan.get("app_id") != record["app_id"] or type(plan.get("configuration_revision")) is not int or plan["configuration_revision"] != 1 or plan.get("visibility") != "public":
        raise ValueError("IAM has not accepted this exact desired-revision plan")
    canonical_uuid(plan["plan_id"])
    expected_gates = {r["provider"]: sorted(r["scopes"]) for r in source["requests"]}
    expected_gates["honeycomb"] = []
    gates = {g["provider"]: sorted(g["scopes"]) for g in plan["gates"]}
    if len(gates) != len(plan["gates"]) or gates != expected_gates:
        raise ValueError("IAM review gates differ from the original pending requests")
    row = db.execute("SELECT revision,iam_revision,effective_revision,credential_version,visibility,config,effective_config,state FROM applications WHERE plane='production' AND app_id=?", (record["app_id"],)).fetchone()
    if not row or row[:5] != (1, record["iam_revision"], 0, record["credential_version"] or 0, "private") or json.loads(row[5]) != config:
        raise ValueError("Application changed during review handover")
    granted = {item["scope"] for item in record["effective_scopes"]}
    expected_effective = {"iam": [s for s in record["app_scope"]["iam"] if s in granted],
        "external": [s for s in record["app_scope"]["external"] if "obo:" + s["app_id"] + ":" + s["endpoint_id"] in granted]}
    if json.loads(row[6]).get("app_scope") != expected_effective or row[7] != ("active" if record["availability"] == "verified" else "disabled"):
        raise ValueError("Effective permissions or availability changed during handover")
    publication = db.execute("SELECT state,plan_id,config_snapshot FROM publication_requests WHERE id=? AND plane='production' AND app_id=? AND revision=1", (request_id, record["app_id"])).fetchone()
    if not publication or publication[:2] != ("awaiting_scope_review", plan["plan_id"]) or json.loads(publication[2]) != config:
        raise ValueError("Ordinary Honeycomb planning must finish before link handover")
    stored = db.execute("SELECT provider,scopes,state,decision_id FROM review_gates WHERE request_id=?", (request_id,)).fetchall()
    if {r[0]: sorted(json.loads(r[1])) for r in stored} != gates or any(r[2:] != ("pending", None) for r in stored):
        raise ValueError("Honeycomb gates differ or a decision happened during handover")
    if db.execute("SELECT 1 FROM decisions WHERE request_id=?", (request_id,)).fetchone():
        raise ValueError("Review decision exists; stop rather than overwrite it")
    for request in source["requests"]:
        for message in request["messages"]:
            imported = db.execute("SELECT request_id,actor,provider,message,created_at FROM discussions WHERE id=?", (message["id"],)).fetchone()
            expected = (request_id, message["actor"] or "legacy-iam-system", request["provider"], message["message"], timestamp(message["created_at"]))
            if imported != expected:
                raise ValueError("Original discussion changed or was not preserved")
    now = int(time.time())
    for request in source["requests"]:
        mapping = db.execute("SELECT request_id,provider,source_sha256 FROM legacy_review_requests WHERE plane='production' AND legacy_request_id=?", (request["id"],)).fetchone()
        expected = (request_id, request["provider"], source_hash)
        if mapping and mapping != expected:
            raise ValueError("Old request link is already mapped to a different review")
        if not mapping:
            db.execute("INSERT INTO legacy_review_requests(plane,legacy_request_id,request_id,provider,source_sha256,imported_at) VALUES('production',?,?,?,?,?)",
                       (request["id"], *expected, now))
    if handover[1] != "complete":
        db.execute("UPDATE legacy_review_handovers SET state='complete',completed_at=? WHERE source_sha256=? AND state='prepared'", (now, source_hash))
        db.execute("INSERT INTO audit(id,plane,actor,action,resource,created_at) VALUES(?,'production',?,'iam.review_handover.complete',?,?)",
                   (str(uuid.uuid4()), actor, record["app_id"], now))
    return {"request_id": request_id, "state": "complete", "replayed": handover[1] == "complete", "legacy_request_ids": [r["id"] for r in source["requests"]]}


def read_plan(settings, plan_id):
    canonical_uuid(plan_id)
    base = settings.get("IAM_BASE_URL", "https://backend.iam.teamofsilicons.com").rstrip("/")
    if urllib.parse.urlparse(base).scheme != "https":
        raise ValueError("IAM must use HTTPS")
    request = urllib.request.Request(base + "/api/v1/honeycomb/publication-plans/" + plan_id,
        headers={"Authorization": "Bearer " + settings["IAM_HONEYCOMB_SERVICE_CREDENTIAL"]})
    with urllib.request.build_opener(NoRedirect).open(request, timeout=30) as response:
        return json.load(response)


def encryptor(settings):
    from cryptography.hazmat.primitives.ciphers.aead import AESGCM
    key = bytes.fromhex(settings["HONEYCOMB_ENCRYPTION_KEY"])
    if len(key) != 32:
        raise ValueError("Honeycomb requires a 32-byte encryption key")
    def encrypt(value):
        nonce = os.urandom(12)
        return nonce.hex() + ":" + AESGCM(key).encrypt(nonce, value.encode(), None).hex()
    return encrypt


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--phase", choices=("prepare", "finalize"), required=True)
    p.add_argument("--source", type=Path, required=True)
    p.add_argument("--expected-source-sha256", required=True)
    p.add_argument("--database", type=Path, required=True)
    p.add_argument("--env-file", type=Path, required=True)
    p.add_argument("--actor", required=True)
    p.add_argument("--backup-dir", type=Path)
    p.add_argument("--apply", action="store_true")
    args = p.parse_args()
    if args.source.stat().st_mode & 0o077:
        p.error("The secret-bearing source must be accessible only by its owner")
    raw = args.source.read_bytes()
    digest = hashlib.sha256(raw).hexdigest()
    if digest != args.expected_source_sha256:
        p.error("Source hash differs from the reviewed export")
    source = json.loads(raw)
    validate(source)
    settings = dict(line.split("=", 1) for line in args.env_file.read_text().splitlines()
                    if line.strip() and not line.lstrip().startswith("#"))
    db = sqlite3.connect(args.database.resolve().as_uri() + "?mode=rw", uri=True, timeout=30)
    db.execute("PRAGMA foreign_keys=ON")
    if not args.apply:
        print(encoded({"preview": True, "phase": args.phase, "app_id": source["application"]["app_id"],
                       "source_sha256": digest, "request_count": len(source["requests"]),
                       "message_count": sum(len(r["messages"]) for r in source["requests"]),
                       "visibility": "private", "grants_changed": False}))
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
        # Keep the local lock while comparing the authoritative live identity.
        # The separate IAM writer-closure rollout freezes legacy requests first.
        if read_record(settings, source["application"]["app_id"]) != source["application"]:
            raise RuntimeError("IAM changed since the export; prepare a fresh reviewed snapshot")
        if args.phase == "prepare":
            receipt = prepare(db, source, digest, args.actor, encryptor(settings))
        else:
            plan = db.execute("SELECT p.plan_id FROM legacy_review_handovers h JOIN publication_requests p ON p.id=h.request_id WHERE h.source_sha256=?", (digest,)).fetchone()
            if not plan or not plan[0]:
                raise ValueError("An authorized manager must run the ordinary Honeycomb review-plan operation first")
            receipt = finalize(db, source, digest, args.actor, read_plan(settings, plan[0]))
        db.commit()
    except BaseException:
        db.rollback()
        raise
    print(encoded({"receipt": receipt, "source_sha256": digest, "backup": str(backup)}))


if __name__ == "__main__":
    main()
