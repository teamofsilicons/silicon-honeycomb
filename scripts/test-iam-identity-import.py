#!/usr/bin/env python3
"""Exercise the real Honeycomb schema's offline canonical ownership migration."""
import importlib.util
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


if __name__ == '__main__':
    unittest.main()
