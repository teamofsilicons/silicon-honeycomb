#!/usr/bin/env python3
"""Backfill Honeycomb actor references from a private pre-cutover IAM export.

Stop API/workers before running. This uses one SQLite transaction, rejects missing
or contradictory mappings, and preserves operation/resource IDs and encrypted data.
Application-create hashes change only after verifying their original exact input.
"""
import argparse
import hashlib
import json
import re
import sqlite3
import uuid
from pathlib import Path

HASH_CONTEXT_SCHEMA = '''CREATE TABLE IF NOT EXISTS application_create_hash_contexts (
 operation_id TEXT PRIMARY KEY REFERENCES operations(id) ON DELETE CASCADE,
 application_id TEXT NOT NULL,
 legacy_hash_application_id TEXT NOT NULL CHECK(length(legacy_hash_application_id)=36)
);'''

def import_identities(database, exported):
    rows = exported if isinstance(exported, list) else exported.get('production', []) + exported.get('testing', [])
    mapping = {}
    reverse = {}
    application_mapping = {}
    for row in rows:
        key = (row.get('testing_environment_id') or 'production', row['legacy_id'])
        value = row['public_id']
        if not value or key in mapping and mapping[key] != value:
            raise ValueError('Conflicting canonical IAM identity export')
        canonical_key = (key[0], value)
        if canonical_key in reverse and reverse[canonical_key] != key[1]:
            raise ValueError('Canonical identity assigned to multiple legacy actors')
        reverse[canonical_key] = key[1]
        mapping[key] = value
        if key[0] == 'production' and row.get('kind') == 'application':
            if not re.fullmatch(r'(?:[a-z0-9_-]+>)?[a-z][a-z0-9_-]{0,79}', value):
                raise ValueError('Invalid canonical production application identity')
            application_mapping[key[1]] = value

    def canonical_application(value, app_id=None):
        replacement = application_mapping.get(value, value)
        if not isinstance(replacement, str) or not re.fullmatch(r'(?:[a-z0-9_-]+>)?[a-z][a-z0-9_-]{0,79}', replacement):
            raise ValueError('Export does not cover retained production application identity')
        if app_id is not None and replacement != app_id:
            raise ValueError('Application identity does not match its retained application handle')
        return replacement

    def canonical(plane, value):
        # These are production application credentials even when activity is
        # recorded inside a testing world. Never use the test identity map.
        if isinstance(value, str) and value.startswith('application:'):
            return 'application:' + canonical_application(value.removeprefix('application:'))
        try:
            uuid.UUID(value)
        except (ValueError, TypeError, AttributeError):
            return value
        if (plane, value) not in mapping:
            raise ValueError(f'Identity export does not cover retained actor in plane {plane}')
        return mapping[(plane, value)]

    db = sqlite3.connect(database)
    db.execute('PRAGMA foreign_keys=ON')
    # Explicit semantic columns only: resource UUIDs and user content must never
    # be rewritten just because their bytes match an identity.
    columns = [
        ('operations', 'actor', 'plane'), ('reviews', 'actor', 'plane'),
        ('stars', 'actor', 'plane'), ('reports', 'actor', 'plane'),
        ('audit', 'actor', 'plane'), ('publication_requests', 'requested_by', 'plane'),
        ('publication_intents', 'principal_id', 'plane'),
        ('environments', 'creator', "'production'"),
        ('environment_activity_events', 'actor', 'environment_id'),
        ('discussions', 'actor', '(SELECT plane FROM publication_requests WHERE id=t.request_id)'),
        ('decisions', 'actor', '(SELECT plane FROM publication_requests WHERE id=t.request_id)'),
        ('publication_authorizations', 'principal_id', '(SELECT plane FROM publication_requests WHERE id=t.request_id)'),
    ]
    tables = {r[0] for r in db.execute("SELECT name FROM sqlite_master WHERE type='table'")}
    changed = 0
    try:
        db.execute('BEGIN IMMEDIATE')
        db.execute(HASH_CONTEXT_SCHEMA)
        # Existing create requests include the old application identity in the
        # digest. Translate only when the exact original input can be verified.
        # A purged/edited world may no longer retain it: keep the digest and its
        # per-operation hash context. That context never authenticates an actor.
        if {'operations', 'environments'} <= tables:
            for rowid, operation_id, identity, digest, org, name, description in db.execute("SELECT o.rowid,o.id,substr(o.actor,13),o.request_hash,e.org_id,e.name,e.description FROM operations o JOIN environments e ON e.id=o.resource WHERE o.plane='production' AND o.kind='environment.prepare' AND o.actor LIKE 'application:%'").fetchall():
                replacement = canonical_application(identity)
                if replacement == identity:
                    continue
                request = {'kind': 'application.environment.create', 'application_id': identity, 'org': org, 'name': name, 'description': description}
                def request_hash(value):
                    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=False).encode()).hexdigest()
                if request_hash(request) == digest:
                    request['application_id'] = replacement
                    db.execute('UPDATE operations SET request_hash=? WHERE rowid=?', (request_hash(request), rowid))
                    changed += 1
                else:
                    db.execute('INSERT INTO application_create_hash_contexts(operation_id,application_id,legacy_hash_application_id) VALUES(?,?,?)', (operation_id, replacement, identity))
                    changed += 1
        for table, column, handle in [
            ('environments', 'creator_application_id', 'creator_app'),
            ('environment_application_links', 'application_id', 'app_id'),
        ]:
            if table not in tables:
                continue
            for rowid, value, app_id in db.execute(f'SELECT rowid,{column},{handle} FROM {table} WHERE {column} IS NOT NULL').fetchall():
                if app_id is None:
                    raise ValueError('Retained application ownership has no application handle')
                replacement = canonical_application(value, app_id)
                if replacement != value:
                    db.execute(f'UPDATE {table} SET {column}=? WHERE rowid=?', (replacement, rowid))
                    changed += 1
        for table, column, plane_sql in columns:
            if table not in tables:
                continue
            for rowid, plane, value in db.execute(f'SELECT rowid,{plane_sql},{column} FROM {table} t').fetchall():
                replacement = canonical(plane, value)
                if replacement != value:
                    db.execute(f'UPDATE {table} SET {column}=? WHERE rowid=?', (replacement, rowid))
                    changed += 1
        # Notification envelope actor is metadata; message and arbitrary payload
        # contents stay byte-for-byte unchanged.
        if 'outbox' in tables:
            for rowid, plane, payload in db.execute('SELECT rowid,plane,payload FROM outbox').fetchall():
                document = json.loads(payload)
                if isinstance(document, dict) and isinstance(document.get('actor'), str):
                    replacement = canonical(plane, document['actor'])
                    if replacement != document['actor']:
                        document['actor'] = replacement
                        db.execute('UPDATE outbox SET payload=? WHERE rowid=?', (json.dumps(document, separators=(',', ':')), rowid))
                        changed += 1
        if db.execute('PRAGMA foreign_key_check').fetchone():
            raise ValueError('Foreign-key verification failed after identity import')
        db.commit()
    except BaseException:
        db.rollback()
        raise
    finally:
        db.close()
    return changed


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--database', type=Path, required=True)
    parser.add_argument('--identity-map', type=Path, required=True)
    args = parser.parse_args()
    if not args.database.is_file():
        parser.error('Existing Honeycomb SQLite database required')
    count = import_identities(args.database, json.loads(args.identity_map.read_text()))
    print(f'Updated {count} Honeycomb identity references/digests; resource IDs and encrypted data preserved.')
