#!/usr/bin/env python3
"""Offline, fail-closed migration of Honeycomb IDs. Preview by default.

Stop Honeycomb API/workers and local package maintenance before applying. The
mapping must come from the IAM migration inventory; never guess organization
ownership from a bare app ID. Historical archive bytes and secret material stay
unchanged. --apply requires a backup directory. See docs/IDENTIFIER-MIGRATION.md.
"""
import argparse
import json
import os
from pathlib import Path
import re
import sqlite3
import time
import uuid
import sys
sys.path.insert(0, str(Path(__file__).resolve().parent))
import identifier_holds

HANDLE = re.compile(r"[a-z][a-z0-9_-]{0,79}\Z")
ORG = re.compile(r"[a-z0-9_-]{3,50}\Z")
APP_FIELDS = {'app_id', 'issuer_app_id', 'audience', 'client_id', 'creator_app', 'provider', 'application_id', 'creator_application_id', 'draft_edit_app_id'}
APP_ARRAYS = {'app_ids', 'retired_apps', 'review_providers'}
# These are arbitrary user content or cryptographic material, even inside JSON.
OPAQUE_FIELDS = {'metadata', 'message', 'description', 'name', 'app_name', 'reason',
                 'error', 'webhook_secret', 'app_secret', 'secret', 'token',
                 'testing_key', 'access_token', 'refresh_token', 'payload_hash'}
JSON_COLUMNS = {'config', 'effective_config', 'snapshot', 'body', 'result', 'payload',
                'receipt', 'scopes', 'request', 'configuration', 'config_snapshot', 'request_json'}
ACTOR_COLUMNS = {'actor', 'principal_id', 'requested_by', 'creator', 'authorizing_manager'}
TERMINAL_PUBLICATIONS = {'published', 'denied', 'rejected', 'cancelled', 'withdrawn'}


def historical_system_actor(table, row):
    """Retain observed maintenance attribution, never grant it IAM identity."""
    actor = row.get('actor')
    if table == 'discussions':
        return actor == 'provider-review-guidance'
    if row.get('plane') != 'production':
        return False
    maintenance = {
        ('shubham:user-requested-starter-catalog-recovery', 'iam.catalog.restore'),
        ('operator:user-requested-honeycomb-publication', 'iam.adopt'),
    }
    if table == 'operations':
        return row.get('state') == 'accepted' and (actor, row.get('kind')) in maintenance
    if table == 'audit':
        return (actor, row.get('action')) in maintenance | {
            ('authorized-operator', 'release.placeholder.materialize'),
            ('operator:silicon-production', 'application.catalog.delete'),
            ('authorized-test-reset-operator', 'testing.import.cancel'),
            ('authorized-test-reset-operator', 'testing.application.cancel'),
        }
    return False


def mapping(document):
    apps, owners, reverse, people = {}, {}, {}, {}
    for row in document['applications']:
        old, new, org = row['legacy_id'], row['app_id'], row['org_id']
        if not HANDLE.fullmatch(new) or not ORG.fullmatch(org):
            raise ValueError('Application mapping requires bare app_id and explicit org_id')
        if old != new and (old.count('>') != 1 or old.split('>')[0] != org):
            raise ValueError(f'Legacy application ownership mismatch: {old}')
        if old in apps and (apps[old] != new or owners[old] != org):
            raise ValueError(f'Conflicting application mapping: {old}')
        if new in reverse and reverse[new] != old:
            raise ValueError(f'Application ID collision: {new}; resolve in IAM before migration')
        apps[old], owners[old], reverse[new] = new, org, old
    for row in document.get('identities', []):
        plane = row.get('testing_environment_id') or 'production'
        old, new = row['legacy_id'], row['public_id']
        if not re.fullmatch(r'(?:c:[a-z0-9_-]{3,30}|si:[a-z0-9_-]{3,50})(?:\[[a-z0-9_-]{3,50}\])?', new):
            raise ValueError('Identity mapping must contain canonical typed public IDs')
        if (plane, old) in people and people[plane, old] != new:
            raise ValueError('Conflicting IAM identity mapping')
        if any(p == plane and n == new and o != old for (p, o), n in people.items()):
            raise ValueError('Identity collision in IAM mapping')
        people[plane, old] = new
    return apps, owners, people


def transform(value, apps, people, plane='production', field=''):
    if field in OPAQUE_FIELDS:
        return value
    if isinstance(value, dict):
        value = dict(value)
        if 'local_app_id' in value and 'org_id' in value:
            old = str(value['org_id']) + '>' + str(value.pop('local_app_id'))
            if old not in apps:
                raise ValueError(f'Missing application mapping for configuration: {old}')
            if 'app_id' in value and value['app_id'] not in (old, apps[old]):
                raise ValueError('Configuration app identity is contradictory')
            value['app_id'] = apps[old]
        return {k: transform(v, apps, people, plane, k) for k, v in value.items()}
    if isinstance(value, list):
        return [transform(v, apps, people, plane, 'app_id' if field in APP_ARRAYS else field) for v in value]
    if not isinstance(value, str):
        return value
    if field in APP_FIELDS:
        if value in apps:
            return apps[value]
        if '>' in value:
            raise ValueError(f'Missing application mapping for {field}: {value}')
    if field in ACTOR_COLUMNS or field == 'public_id':
        if value.startswith('application:'):
            identity = value.removeprefix('application:')
            return 'application:' + transform(identity, apps, people, 'production', 'application_id')
        if value in ('', None, 'honeycomb-coordinator', 'honeycomb-retention'):
            return value
        if (plane, value) in people:
            return people[plane, value]
        if value.startswith(('c:', 'si:')):
            return value
        raise ValueError(f'Missing IAM identity mapping in {plane}: {value}')
    if field in ('scope', 'scopes') and value.startswith('obo:'):
        provider, separator, endpoint = value[4:].partition(':')
        if separator:
            return 'obo:' + transform(provider, apps, people, plane, 'provider') + ':' + endpoint
    return value


def migrate_database(db, document, apply=False):
    apps, owners, people = mapping(document)
    tables = {r[0] for r in db.execute("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'")}
    if 'migrated_release_identities' not in tables:
        raise ValueError('Apply Honeycomb schema migration 0026 before this offline migration')
    held = identifier_holds.verify(db, lambda value, column, plane: apps.get(value, value) if column == 'resource' else transform(value, apps, people, plane, column))
    # Encrypted operation bodies are immutable replay records. Drain them before
    # cutover instead of silently rewriting requests or their idempotency hashes.
    for table, condition in [('operations', "state='pending'"), ('outbox', "state IN ('pending','sending','uncertain')"),
                             ('decisions', "state='pending'"), ('publication_requests',
                              "state NOT IN ('published','denied','rejected','cancelled','withdrawn','held_identifier_migration')")]:
        if table in tables and db.execute(f'SELECT 1 FROM {table} WHERE {condition} LIMIT 1').fetchone():
            raise ValueError(f'Drain or explicitly cancel in-flight {table} before ID migration')
    existing = db.execute('SELECT plane,app_id,org_id FROM applications').fetchall()
    claimed = set()
    for plane, old, org in existing:
        if old in apps:
            if owners[old] != org:
                raise ValueError(f'Catalog owning organization differs from IAM: {old}')
            new = apps[old]
        elif HANDLE.fullmatch(old):
            new = old
        else:
            raise ValueError(f'Missing IAM app mapping: {old}')
        if (plane, new) in claimed:
            raise ValueError(f'Application collision in {plane}: {new}')
        claimed.add((plane, new))
    changed = 0
    db.execute('PRAGMA foreign_keys=ON')
    db.execute('BEGIN IMMEDIATE')
    db.execute('PRAGMA defer_foreign_keys=ON')
    try:
        # Bind retained create digests to their original exact application input.
        # Existing UUID contexts take precedence; hashes and encrypted requests stay unchanged.
        for operation_id, actor in db.execute("SELECT id,actor FROM operations WHERE plane='production' AND kind='environment.prepare' AND actor LIKE 'application:%'").fetchall():
            old = actor.removeprefix('application:')
            if old in apps and old != apps[old]:
                db.execute('INSERT INTO application_create_hash_contexts(operation_id,application_id,legacy_hash_application_id) VALUES(?,?,?) ON CONFLICT(operation_id) DO UPDATE SET application_id=excluded.application_id', (operation_id, apps[old], old))
        for plane, old, org in existing:
            if old in apps and old != apps[old]:
                db.execute('INSERT INTO migrated_release_identities SELECT plane,app_id,channel,version,? FROM releases WHERE plane=? AND app_id=? ON CONFLICT DO NOTHING', (old, plane, old))
        for table in sorted(tables):
            if table.startswith(('_', 'catalog_search')):
                continue
            columns = [r[1] for r in db.execute(f'PRAGMA table_info("{table}")')]
            plane_column = 'plane' if 'plane' in columns else 'environment_id' if 'environment_id' in columns else None
            names = ['rowid'] + columns
            for raw in db.execute(f'SELECT rowid,* FROM "{table}"').fetchall():
                row = dict(zip(names, raw))
                plane = row.get(plane_column) or 'production'
                terminal_review = table == 'publication_requests' and row['state'] in TERMINAL_PUBLICATIONS
                if table in {'discussions', 'decisions', 'publication_authorizations', 'review_gates'}:
                    parent = db.execute('SELECT plane,state FROM publication_requests WHERE id=?', (row['request_id'],)).fetchone()
                    if parent:
                        plane = parent[0]
                        terminal_review = parent[1] in TERMINAL_PUBLICATIONS
                for column in columns:
                    value, new = row[column], row[column]
                    if value is None:
                        continue
                    if terminal_review and (column == 'provider' or column in JSON_COLUMNS):
                        # Final review evidence can contain both a coordinator
                        # role and an app provider with the same new spelling.
                        # Retain its original authority namespace and bytes.
                        pass
                    elif column == 'actor' and historical_system_actor(table, row):
                        pass
                    elif column in APP_FIELDS or column in ACTOR_COLUMNS:
                        new = transform(value, apps, people, plane, column)
                    elif column == 'resource' and value in apps:
                        new = apps[value]
                    elif column in JSON_COLUMNS and isinstance(value, str) and (table, row.get(identifier_holds.PRIMARY.get(table, 'id'))) not in held and table != 'operations' and not (table == 'downloads' and column == 'receipt'):
                        content = json.loads(value)
                        if table == 'drafts' and column == 'body' and isinstance(content, dict) and 'local_app_id' in content:
                            # A draft is an unregistered proposal. Its requested
                            # handle is not an IAM-owned application yet.
                            org, proposed = content.get('org_id'), content['local_app_id']
                            if org != row['org_id'] or not isinstance(proposed, str) or (proposed and not HANDLE.fullmatch(proposed)):
                                raise ValueError('Draft application handle or owning organization is invalid')
                            old = org + '>' + proposed
                            target = apps.get(old, proposed)
                            if content.get('app_id', target) not in (old, target):
                                raise ValueError('Draft application identity is contradictory')
                            content = {**content, 'app_id': target}
                            del content['local_app_id']
                        new = json.dumps(transform(content, apps, people, plane, column), separators=(',', ':'))
                    if new != value:
                        db.execute(f'UPDATE "{table}" SET "{column}"=? WHERE rowid=?', (new, row['rowid']))
                        changed += 1
        identifier_holds.verify(db, lambda value, column, plane: apps.get(value, value) if column == 'resource' else transform(value, apps, people, plane, column))
        if 'catalog_search' in tables:
            db.execute('DELETE FROM catalog_search')
            db.execute("INSERT INTO catalog_search(plane,app_id,variant,name,description) SELECT plane,app_id,'desired',name,description FROM applications")
            db.execute("INSERT INTO catalog_search(plane,app_id,variant,name,description) SELECT plane,app_id,'effective',json_extract(effective_config,'$.name'),json_extract(effective_config,'$.description') FROM applications WHERE iam_revision>0")
        if db.execute('PRAGMA foreign_key_check').fetchone():
            raise ValueError('Foreign key check failed; migration rolled back')
        if apply:
            db.commit()
        else:
            db.rollback()
    except BaseException:
        db.rollback()
        raise
    return changed


def migrate_installed(path, document, apply=False):
    apps, _, _ = mapping(document)
    records = json.loads(Path(path).read_text())
    result = {}
    for key, record in records.items():
        old = record['app_id']
        if old not in apps and '>' in old:
            raise ValueError(f'Missing installed app mapping: {old}')
        app = apps.get(old, old)
        expected = old + ('>test' if record.get('channel', 'prod') == 'dev' else '')
        if key != expected:
            raise ValueError('Installed registry key disagrees with recorded identity/channel')
        new_key = app + ('>test' if record.get('channel', 'prod') == 'dev' else '')
        if new_key in result:
            raise ValueError(f'Installed package collision: {new_key}')
        result[new_key] = {**record, 'app_id': app}
    if apply:
        temporary = Path(str(path) + '.migration-' + str(uuid.uuid4()))
        fd = os.open(temporary, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
        with os.fdopen(fd, 'w') as output:
            json.dump(result, output, indent=2)
            output.flush()
            os.fsync(output.fileno())
        os.replace(temporary, path)
    return sum(key not in records or value != records[key] for key, value in result.items())


def main():
    p = argparse.ArgumentParser(description=__doc__)
    target = p.add_mutually_exclusive_group(required=True)
    target.add_argument('--database', type=Path)
    target.add_argument('--installed', type=Path)
    p.add_argument('--map', type=Path, required=True)
    p.add_argument('--apply', action='store_true')
    p.add_argument('--backup-dir', type=Path)
    args = p.parse_args()
    source = args.database or args.installed
    if not source.is_file():
        p.error('An existing database or installed registry is required')
    if args.apply and not args.backup_dir:
        p.error('--apply requires --backup-dir')
    document = json.loads(args.map.read_text())
    os.umask(0o077)
    db = sqlite3.connect(source.resolve().as_uri() + '?mode=rw', uri=True) if args.database else None
    try:
        count = migrate_database(db, document) if db else migrate_installed(source, document)
        if args.apply:
            args.backup_dir.mkdir(parents=True, exist_ok=True)
            backup = args.backup_dir / (time.strftime('%Y%m%dT%H%M%SZ', time.gmtime()) + '-' + str(uuid.uuid4()) + '-' + source.name)
            if db:
                with sqlite3.connect(backup) as destination:
                    db.backup(destination)
                    if destination.execute('PRAGMA integrity_check').fetchone()[0] != 'ok':
                        raise ValueError('Backup integrity verification failed')
                count = migrate_database(db, document, True)
            else:
                backup.write_bytes(source.read_bytes())
                count = migrate_installed(source, document, True)
        print(json.dumps({'applied': args.apply, 'changed_fields': count}))
    finally:
        if db:
            db.close()

if __name__ == '__main__':
    main()
