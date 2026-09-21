#!/usr/bin/env python3
"""Backfill Honeycomb actor references from a private pre-cutover IAM export.

Stop API/workers before running. This uses one SQLite transaction, rejects missing
or contradictory mappings, and preserves operation/resource IDs and request hashes.
"""
import argparse
import json
import sqlite3
import uuid
from pathlib import Path


def import_identities(database, exported):
    rows = exported if isinstance(exported, list) else exported.get('production', []) + exported.get('testing', [])
    mapping = {}
    reverse = {}
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

    def canonical(plane, value):
        try:
            uuid.UUID(value)
        except (ValueError, TypeError, AttributeError):
            return value
        if (plane, value) not in mapping:
            raise ValueError(f'Identity export does not cover retained actor in plane {plane}')
        return mapping[(plane, value)]

    db = sqlite3.connect(database)
    db.execute('PRAGMA foreign_keys=ON')
    # Explicit semantic columns only: application IDs, resource UUIDs and user
    # content must never be rewritten just because their bytes match an identity.
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
    print(f'Updated {count} Honeycomb identity references; resource IDs and operation hashes preserved.')
