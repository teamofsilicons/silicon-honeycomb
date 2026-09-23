#!/usr/bin/env python3
"""Exercise the real Honeycomb schema's offline canonical ownership migration."""
import importlib.util
import hashlib
import json
from pathlib import Path
import sqlite3
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('identity_import', ROOT / 'scripts/import-iam-identities.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
OLD = '11111111-1111-4111-8111-111111111111'
TEST_OLD = '22222222-2222-4222-8222-222222222222'
ENV = '33333333-3333-4333-8333-333333333333'
APP_OLD = '44444444-4444-4444-8444-444444444444'


class IdentityImportTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.db = Path(self.temp.name) / 'honeycomb.sqlite'
        with sqlite3.connect(self.db) as db:
            for migration in sorted((ROOT / 'crates/server/migrations').glob('*.sql')):
                db.executescript(migration.read_text())
            for plane, actor, key in [('production', OLD, OLD), (ENV, TEST_OLD, TEST_OLD)]:
                db.execute("INSERT INTO operations(id,plane,actor,idempotency_key,kind,resource,request_hash,revision,created_at) VALUES(?,?,?,'retry','configure',?,'unchanged-hash',1,1)", (key, plane, actor, OLD))
                db.execute("INSERT INTO outbox(id,plane,event_key,kind,payload,created_at) VALUES(?,?,?,'review',?,1)", (key, plane, key, json.dumps({'actor': actor, 'message': OLD})))
        self.mapping = {'production': [{'legacy_id': OLD, 'public_id': 'saket', 'testing_environment_id': None}], 'testing': [{'legacy_id': TEST_OLD, 'public_id': 'saket', 'testing_environment_id': ENV}]}

    def tearDown(self):
        self.temp.cleanup()

    def test_scoped_ownership_resource_ids_retry_hashes_and_messages(self):
        self.assertEqual(module.import_identities(self.db, self.mapping), 4)
        self.assertEqual(module.import_identities(self.db, self.mapping), 0)
        with sqlite3.connect(self.db) as db:
            rows = db.execute('SELECT id,plane,actor,resource,request_hash FROM operations ORDER BY plane').fetchall()
            self.assertEqual({row[0] for row in rows}, {OLD, TEST_OLD})
            self.assertTrue(all(row[2:] == ('saket', OLD, 'unchanged-hash') for row in rows))
            for (payload,) in db.execute('SELECT payload FROM outbox'):
                self.assertEqual(json.loads(payload), {'actor': 'saket', 'message': OLD})

    def test_incomplete_export_rolls_back_every_change(self):
        with self.assertRaises(ValueError):
            module.import_identities(self.db, {'production': self.mapping['production']})
        with sqlite3.connect(self.db) as db:
            self.assertEqual({row[0] for row in db.execute('SELECT actor FROM operations')}, {OLD, TEST_OLD})

    def test_duplicate_canonical_identity_is_rejected(self):
        self.mapping['production'].append({'legacy_id': TEST_OLD, 'public_id': 'saket', 'testing_environment_id': None})
        with self.assertRaises(ValueError):
            module.import_identities(self.db, self.mapping)

    def seed_application(self):
        self.mapping['production'].append({'legacy_id': APP_OLD, 'kind': 'application', 'public_id': 'alpha>app', 'testing_environment_id': None})
        request = {'kind': 'application.environment.create', 'application_id': APP_OLD, 'org': 'alpha', 'name': 'Retained world', 'description': 'Keep unchanged'}
        digest = hashlib.sha256(json.dumps(request, sort_keys=True, separators=(',', ':')).encode()).hexdigest()
        with sqlite3.connect(self.db) as db:
            db.execute("INSERT INTO environments(id,org_id,creator,creator_app,creator_application_id,name,description,encrypted_key,key_hash,state,created_at,last_activity) VALUES(?,'alpha',?,'alpha>app',?,'Retained world','Keep unchanged','opaque-root-ciphertext','root-hash','ready',1,1)", (ENV, 'application:'+APP_OLD, APP_OLD))
            db.execute("INSERT INTO environment_application_links(environment_id,application_id,app_id,last_activity,state) VALUES(?,?,'alpha>app',1,'ready')", (ENV, APP_OLD))
            db.execute("INSERT INTO operations(id,plane,actor,idempotency_key,kind,resource,request_hash,revision,created_at) VALUES('app-create','production',?,'app-retry','environment.prepare',?,?,1,1)", ('application:'+APP_OLD, ENV, digest))
            db.execute("INSERT INTO environment_activity_events(environment_id,idempotency_key,actor,app_id,generation,key_version,received_at) VALUES(?,'application-event',?,'alpha>app',1,1,1)", (ENV, 'application:'+APP_OLD))
        return request, digest

    def test_application_ownership_links_activity_and_exact_create_retry(self):
        request, _ = self.seed_application()
        request['application_id'] = 'alpha>app'
        expected = hashlib.sha256(json.dumps(request, sort_keys=True, separators=(',', ':')).encode()).hexdigest()
        module.import_identities(self.db, self.mapping)
        self.assertEqual(module.import_identities(self.db, self.mapping), 0)
        with sqlite3.connect(self.db) as db:
            self.assertEqual(db.execute('SELECT id,creator,creator_app,creator_application_id,encrypted_key,key_hash,name,description FROM environments').fetchone(), (ENV, 'application:alpha>app', 'alpha>app', 'alpha>app', 'opaque-root-ciphertext', 'root-hash', 'Retained world', 'Keep unchanged'))
            self.assertEqual(db.execute('SELECT environment_id,application_id,app_id,state FROM environment_application_links').fetchone(), (ENV, 'alpha>app', 'alpha>app', 'ready'))
            self.assertEqual(db.execute("SELECT actor,resource,request_hash FROM operations WHERE id='app-create'").fetchone(), ('application:alpha>app', ENV, expected))
            self.assertEqual(db.execute('SELECT actor FROM environment_activity_events').fetchone(), ('application:alpha>app',))

    def test_application_mapping_must_be_production_and_match_retained_handle(self):
        self.seed_application()
        record = self.mapping['production'].pop()
        record['testing_environment_id'] = ENV
        self.mapping['testing'].append(record)
        with self.assertRaises(ValueError):
            module.import_identities(self.db, self.mapping)
        record['testing_environment_id'] = None
        self.mapping['production'].append(dict(record, public_id='alpha>other'))
        with self.assertRaises(ValueError):
            module.import_identities(self.db, self.mapping)
        with sqlite3.connect(self.db) as db:
            self.assertEqual(db.execute('SELECT creator_application_id FROM environments').fetchone(), (APP_OLD,))

    def test_unrecoverable_create_input_keeps_original_hash_and_bound_context(self):
        _, digest = self.seed_application()
        with sqlite3.connect(self.db) as db:
            db.execute("UPDATE environments SET name='Purged environment',description='' ")
        module.import_identities(self.db, self.mapping)
        with sqlite3.connect(self.db) as db:
            self.assertEqual(db.execute("SELECT request_hash FROM operations WHERE id='app-create'").fetchone(), (digest,))
            self.assertEqual(db.execute('SELECT operation_id,application_id,legacy_hash_application_id FROM application_create_hash_contexts').fetchone(), ('app-create', 'alpha>app', APP_OLD))
        self.assertEqual(module.import_identities(self.db, self.mapping), 0)

    def test_offline_hash_context_schema_matches_startup_migration(self):
        migration = (ROOT / 'crates/server/migrations/0023_application_create_hash_contexts.sql').read_text()
        self.assertEqual(module.HASH_CONTEXT_SCHEMA.strip(), migration.strip())


if __name__ == '__main__':
    unittest.main()
