#!/usr/bin/env python3
"""Preview/archive exact outstanding work for a coordinated offline cutover. Never replay."""
import argparse
import json
import os
from pathlib import Path
import sqlite3
import uuid
import identifier_holds as holds


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--database',type=Path,required=True)
    parser.add_argument('--manifest',type=Path,required=True)
    parser.add_argument('--capture',action='store_true',help='write hashes-only manifest for review; database read-only')
    parser.add_argument('--apply',action='store_true')
    parser.add_argument('--stopped',action='store_true')
    parser.add_argument('--backup-dir',type=Path)
    args=parser.parse_args()
    os.umask(0o077)
    if args.capture and args.apply:parser.error('capture and apply are mutually exclusive')
    if args.apply and (not args.stopped or not args.backup_dir):parser.error('apply requires stopped writers and backup-dir')
    db=sqlite3.connect(args.database.resolve().as_uri()+('?mode=ro' if args.capture else '?mode=rw'),uri=True)
    try:
        if args.capture:
            manifest=holds.inventory(db)
            with args.manifest.open('x') as output:json.dump(manifest,output,indent=2)
            print(json.dumps({'captured':len(manifest['records'])}))
            return
        manifest=json.loads(args.manifest.read_text())
        count=holds.hold(db,manifest)
        if args.apply:
            args.backup_dir.mkdir(mode=0o700,parents=True,exist_ok=True)
            backup=args.backup_dir/(str(uuid.uuid4())+'-before-hold.db')
            with sqlite3.connect(backup) as destination:
                db.backup(destination)
                if destination.execute('PRAGMA integrity_check').fetchone()!=('ok',):raise ValueError('Backup integrity check failed')
            count=holds.hold(db,manifest,True)
        print(json.dumps({'applied':args.apply,'held_records':count,'state':holds.STATE}))
    finally:db.close()

if __name__=='__main__':main()
