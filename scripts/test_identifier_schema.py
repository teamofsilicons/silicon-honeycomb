import importlib.util
import json
from pathlib import Path
import sqlite3
import tempfile
import unittest
spec = importlib.util.spec_from_file_location('migration', Path(__file__).with_name('migrate_identifier_schema.py'))
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
MAP = {'applications':[{'legacy_id':'tos>hello','app_id':'hello','org_id':'tos'}],
       'identities':[{'legacy_id':'saket','public_id':'c:saket'}]}

class IdentifierMigration(unittest.TestCase):
    def database(self):
        db = sqlite3.connect(':memory:')
        for path in sorted((Path(__file__).parents[1] / 'crates/server/migrations').glob('*.sql')):
            db.executescript(path.read_text())
        config = json.dumps({'org_id':'tos','local_app_id':'hello','name':'Hello',
                             'metadata':{'app_id':'tos>hello'},'description':'saket tos>hello'})
        db.execute("INSERT INTO applications(plane,app_id,org_id,name,description,config,effective_config,webhook_secret,created_at,updated_at) VALUES('production','tos>hello','tos','Hello','saket',?,?,'encrypted:secret',1,1)", (config,config))
        db.execute("INSERT INTO releases VALUES('production','tos>hello','prod','1.0.0','unchanged-checksum',7,'opaque:tos>hello:ref',1)")
        db.execute("INSERT INTO stars VALUES('production','tos>hello','saket')")
        db.execute("INSERT INTO downloads VALUES('production','tos>hello','opaque-download-token',1)")
        db.execute("INSERT INTO operations(id,plane,actor,idempotency_key,kind,resource,request_hash,revision,state,created_at) VALUES('op','production','saket','old-key','configure','tos>hello','old-hash',1,'accepted',1)")
        db.commit()
        self.addCleanup(db.close)
        return db
    def test_preview_apply_repeat_and_opaque_preservation(self):
        db = self.database()
        self.assertGreater(m.migrate_database(db, MAP), 0)
        self.assertEqual(db.execute('SELECT app_id FROM applications').fetchone()[0], 'tos>hello')
        m.migrate_database(db, MAP, True)
        app, config, secret = db.execute('SELECT app_id,config,webhook_secret FROM applications').fetchone()
        self.assertEqual(app, 'hello')
        self.assertEqual(json.loads(config)['app_id'], 'hello')
        self.assertEqual(json.loads(config)['metadata']['app_id'], 'tos>hello')
        self.assertEqual(secret, 'encrypted:secret')
        self.assertEqual(db.execute('SELECT app_id,actor FROM stars').fetchone(), ('hello','c:saket'))
        self.assertEqual(db.execute('SELECT storage_ref,sha256 FROM releases').fetchone(), ('opaque:tos>hello:ref','unchanged-checksum'))
        self.assertEqual(db.execute('SELECT app_id,legacy_app_id FROM migrated_release_identities').fetchone(), ('hello','tos>hello'))
        self.assertEqual(db.execute('SELECT request_hash FROM operations').fetchone()[0], 'old-hash')
        self.assertEqual(db.execute('SELECT DISTINCT app_id FROM catalog_search').fetchall(), [('hello',)])
        self.assertEqual(m.migrate_database(db, MAP, True), 0)
    def test_application_owners_and_exact_historical_hash_inputs_are_retained(self):
        db = self.database()
        for name in ('qualified', 'old-uuid'):
            db.execute("INSERT INTO operations(id,plane,actor,idempotency_key,kind,resource,request_hash,revision,state,created_at) VALUES(?,'production','application:tos>hello',?,'environment.prepare','world','unchanged-create-hash',1,'accepted',1)", (name,name))
        legacy_uuid='44444444-4444-4444-8444-444444444444'
        db.execute('INSERT INTO application_create_hash_contexts VALUES(?,?,?)',('old-uuid','tos>hello',legacy_uuid))
        db.commit()
        m.migrate_database(db, MAP, True)
        self.assertEqual(db.execute("SELECT actor,request_hash FROM operations WHERE id='qualified'").fetchone(),('application:hello','unchanged-create-hash'))
        self.assertEqual(db.execute('SELECT application_id,legacy_hash_application_id FROM application_create_hash_contexts WHERE operation_id=?',('qualified',)).fetchone(),('hello','tos>hello'))
        self.assertEqual(db.execute('SELECT application_id,legacy_hash_application_id FROM application_create_hash_contexts WHERE operation_id=?',('old-uuid',)).fetchone(),('hello',legacy_uuid))
        self.assertEqual(m.migrate_database(db, MAP, True),0)
    def test_collision_and_owner_mismatch_roll_back(self):
        db = self.database()
        for invalid in [
            {'applications':MAP['applications']+[{'legacy_id':'other>hello','app_id':'hello','org_id':'other'}]},
            {'applications':[{'legacy_id':'tos>hello','app_id':'hello','org_id':'other'}]},
            {'applications':MAP['applications'],'identities':[]},
        ]:
            with self.assertRaises(ValueError): m.migrate_database(db, invalid, True)
            self.assertEqual(db.execute('SELECT app_id FROM applications').fetchone()[0], 'tos>hello')
    def test_pending_work_blocks_cutover(self):
        db = self.database()
        db.execute("UPDATE operations SET state='pending'")
        db.commit()
        with self.assertRaisesRegex(ValueError, 'in-flight'): m.migrate_database(db, MAP, True)
    def test_installed_registry_preserves_physical_paths_and_channels(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp)/'installed.json'
            record = {'app_id':'tos>hello','channel':'dev','directory':'/old/packages/tos/hello/dev/1.0.0-abcd','commands':{'hello':'/old/bin/hello'},'sha256':'unchanged'}
            path.write_text(json.dumps({'tos>hello>test':record}))
            self.assertEqual(m.migrate_installed(path, MAP), 1)
            self.assertIn('tos>hello>test',json.loads(path.read_text()))
            m.migrate_installed(path, MAP, True)
            migrated = json.loads(path.read_text())['hello>test']
            self.assertEqual(migrated['directory'],record['directory'])
            self.assertEqual(migrated['commands'],record['commands'])
            path.write_text(json.dumps({'tos>hello>test':record,'hello>test':{**record,'app_id':'hello'}}))
            with self.assertRaisesRegex(ValueError,'collision'):m.migrate_installed(path, MAP, True)

if __name__ == '__main__':unittest.main()
