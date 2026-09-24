"""Verified offline holds; original row values remain immutable audit evidence."""
import base64
import hashlib
import json
import sqlite3
import time

STATE = 'held_identifier_migration'
TABLE = '_identifier_cutover_holds'
PRIMARY = {'operations':'id', 'outbox':'id', 'publication_requests':'id', 'management_requests':'operation_id'}
ROUTING = {'operations': {'actor','resource'}, 'publication_requests': {'app_id','requested_by'}, 'outbox': set(), 'management_requests': {'app_id'}}
OPEN = {'operations': "state='pending'", 'outbox': "state IN ('pending','sending','uncertain')", 'publication_requests': "state NOT IN ('published','denied','rejected','cancelled','withdrawn')"}


def packed(row):
    return json.dumps({k: {'__blob__':base64.b64encode(v).decode()} if isinstance(v, bytes) else v for k,v in row.items()}, sort_keys=True, separators=(',',':'), ensure_ascii=True)


def digest(value):
    return hashlib.sha256(value.encode()).hexdigest()


def row(db, table, key):
    cursor = db.execute(f'SELECT * FROM "{table}" WHERE "{PRIMARY[table]}"=?', (key,))
    values = cursor.fetchone()
    if values is None:
        raise ValueError('Hold source row is missing')
    return dict(zip((c[0] for c in cursor.description), values))


def inventory(db):
    records=[]
    for table, condition in OPEN.items():
        for (key,) in db.execute(f'SELECT id FROM "{table}" WHERE {condition} AND state!=? ORDER BY id', (STATE,)):
            item=row(db,table,key)
            if item['plane']!='production':
                raise ValueError('Testing work remains; complete its supported lifecycle first')
            records.append({'table':table,'id':key,'sha256':digest(packed(item))})
            if table=='operations':
                if item.get('lease_until',0) and item['lease_until'] > time.time():
                    raise ValueError('An operation still has a live lease; stop writers and wait for expiry')
                for (child,) in db.execute('SELECT operation_id FROM management_requests WHERE operation_id=?',(key,)):
                    records.append({'table':'management_requests','id':child,'sha256':digest(packed(row(db,'management_requests',child)))})
            if table=='publication_requests':
                for child in ['decisions','review_gates','publication_authorizations']:
                    if db.execute(f'SELECT 1 FROM {child} WHERE request_id=? LIMIT 1',(key,)).fetchone():
                        raise ValueError('Publication has authority/decision children; reconcile them before holding')
                if item.get('activation_operation'):
                    raise ValueError('Publication activation has started; reconcile its archive transaction first')
    return {'version':1,'records':sorted(records,key=lambda r:(r['table'],r['id']))}


def hold(db, manifest, apply=False):
    db.execute('BEGIN IMMEDIATE')
    try:
        if manifest != inventory(db):
            raise ValueError('Hold manifest does not match the complete frozen inventory')
        db.execute(f'''CREATE TABLE IF NOT EXISTS {TABLE} (
            table_name TEXT NOT NULL, row_id TEXT NOT NULL, original_row TEXT NOT NULL,
            original_sha256 TEXT NOT NULL, held_at INTEGER NOT NULL, reason TEXT NOT NULL,
            PRIMARY KEY(table_name,row_id))''')
        for item in manifest['records']:
            table,key=item['table'],item['id']
            original=packed(row(db,table,key))
            db.execute(f'INSERT INTO {TABLE} VALUES(?,?,?,?,?,?)',(table,key,original,digest(original),int(time.time()),'Coordinated public identifier cutover; operator reconciliation required before any replay'))
            if table!='management_requests':
                db.execute(f'UPDATE "{table}" SET state=? WHERE id=?',(STATE,key))
        # Block mutation of evidence (including manual retry paths) while allowing
        # only the authoritative mapper's typed routing references to change.
        for table, primary in PRIMARY.items():
            immutable=[c[1] for c in db.execute(f'PRAGMA table_info("{table}")') if c[1] not in ROUTING[table]]
            changed=' OR '.join(f'NEW."{column}" IS NOT OLD."{column}"' for column in immutable)
            held=f"EXISTS(SELECT 1 FROM {TABLE} WHERE table_name='{table}' AND row_id=OLD.\"{primary}\")"
            db.execute(f'''CREATE TRIGGER IF NOT EXISTS _identifier_hold_{table}_update BEFORE UPDATE ON "{table}"
                WHEN {held} AND ({changed}) BEGIN SELECT RAISE(ABORT,'identifier migration hold requires operator reconciliation'); END''')
            db.execute(f'''CREATE TRIGGER IF NOT EXISTS _identifier_hold_{table}_delete BEFORE DELETE ON "{table}"
                WHEN {held} BEGIN SELECT RAISE(ABORT,'identifier migration hold evidence cannot be deleted'); END''')
        verify(db)
        if apply: db.commit()
        else: db.rollback()
    except BaseException:
        db.rollback()
        raise
    return len(manifest['records'])


def verify(db, transform=None):
    if not db.execute("SELECT 1 FROM sqlite_master WHERE type='table' AND name=?",(TABLE,)).fetchone():
        for table in OPEN:
            if db.execute(f'SELECT 1 FROM {table} WHERE state=? LIMIT 1',(STATE,)).fetchone():
                raise ValueError('Held state has no verified original evidence')
        return set()
    verified=set()
    for table,key,original,sha in db.execute(f'SELECT table_name,row_id,original_row,original_sha256 FROM {TABLE}'):
        if table not in PRIMARY or digest(original)!=sha:
            raise ValueError('Hold evidence hash or table is invalid')
        expected=json.loads(original)
        if table!='management_requests':expected['state']=STATE
        actual=json.loads(packed(row(db,table,key)))
        for column in ROUTING[table]:
            if actual[column] != expected[column]:
                if transform is None or actual[column] != transform(expected[column],column,expected.get('plane','production')):
                    raise ValueError('Held routing reference is not the verified IAM mapping')
                expected[column]=actual[column]
        if actual!=expected:
            raise ValueError('Held immutable row changed; restore original evidence')
        verified.add((table,key))
    for table in OPEN:
        for (key,) in db.execute(f'SELECT id FROM {table} WHERE state=?',(STATE,)):
            if (table,key) not in verified:raise ValueError('Held row has no evidence')
    return verified
