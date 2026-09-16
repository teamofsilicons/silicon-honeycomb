\set ON_ERROR_STOP on
BEGIN;
SET LOCAL statement_timeout='120s';
SET LOCAL lock_timeout='15s';
-- Freeze IAM writers while computing exact row dependencies. Read-only traffic can continue.
DO $$ DECLARE r record; BEGIN
 FOR r IN SELECT oid::regclass AS rel FROM pg_class WHERE relnamespace='iam'::regnamespace AND relkind IN ('r','p') ORDER BY oid LOOP
  EXECUTE format('LOCK TABLE %s IN SHARE ROW EXCLUSIVE MODE',r.rel);
 END LOOP;
END $$;
CREATE TEMP TABLE reset_apps AS SELECT id,app_id FROM iam.applications WHERE app_id<>'tos>honeycomb';
DO $$ BEGIN
 IF (SELECT count(*) FROM iam.applications WHERE app_id='tos>honeycomb' AND id='01a0a751-69c0-70b2-8456-96806349f999' AND review_status='verified')<>1 THEN RAISE EXCEPTION 'Honeycomb identity precondition failed'; END IF;
 IF EXISTS(SELECT 1 FROM reset_apps WHERE app_id NOT LIKE 'tos>%') THEN RAISE EXCEPTION 'Unexpected owning organization'; END IF;
 IF (SELECT array_agg(app_id ORDER BY app_id) FROM reset_apps) IS DISTINCT FROM ARRAY['tos>briefcase','tos>browser','tos>commit','tos>dm','tos>hook','tos>iam','tos>remind','tos>spacestation','tos>starter','tos>waveform']::text[] THEN RAISE EXCEPTION 'Application inventory changed'; END IF;
END $$;
-- Preserve security history and environment ownership while detaching removed apps.
CREATE TEMP TABLE reset_protected(rel oid PRIMARY KEY, fingerprint text NOT NULL);
DO $$ DECLARE r record; value text; BEGIN
 FOR r IN SELECT oid,oid::regclass AS rel FROM pg_class WHERE relnamespace='iam'::regnamespace AND relkind='r' ORDER BY oid LOOP
  EXECUTE format('SELECT md5(COALESCE(jsonb_agg(to_jsonb(t) ORDER BY to_jsonb(t)::text),''[]''::jsonb)::text) FROM %s t WHERE to_jsonb(t)::text LIKE ''%%01a0a751-69c0-70b2-8456-96806349f999%%''',r.rel) INTO value;
  INSERT INTO reset_protected VALUES(r.oid,value);
 END LOOP;
END $$;
CREATE TEMP TABLE reset_core AS
SELECT 'iam.carbons'::regclass AS rel,md5(COALESCE(jsonb_agg(to_jsonb(t) ORDER BY id),'[]'::jsonb)::text) fingerprint FROM iam.carbons t
UNION ALL SELECT 'iam.organizations'::regclass,md5(COALESCE(jsonb_agg(to_jsonb(t) ORDER BY id),'[]'::jsonb)::text) FROM iam.organizations t
UNION ALL SELECT 'iam.organization_memberships'::regclass,md5(COALESCE(jsonb_agg(to_jsonb(t) ORDER BY id),'[]'::jsonb)::text) FROM iam.organization_memberships t;
CREATE TEMP TABLE reset_testing AS SELECT id,created_by_application_id FROM iam.testing_environments;
ALTER TABLE iam.audit_events DISABLE TRIGGER audit_events_append_only;
UPDATE iam.audit_events SET metadata=metadata||jsonb_build_object('operator_reset_application_id',application_id),application_id=NULL WHERE application_id IN (SELECT id FROM reset_apps);
UPDATE iam.audit_events SET metadata=metadata||jsonb_build_object('operator_reset_actor_id',actor_principal_id),actor_principal_id=NULL,actor_kind=NULL WHERE actor_principal_id IN (SELECT id FROM reset_apps);
ALTER TABLE iam.audit_events ENABLE TRIGGER audit_events_append_only;
ALTER TABLE iam.authentication_events DISABLE TRIGGER authentication_events_append_only;
UPDATE iam.authentication_events SET metadata=metadata||jsonb_build_object('operator_reset_application_id',application_id),application_id=NULL WHERE application_id IN (SELECT id FROM reset_apps);
UPDATE iam.authentication_events SET metadata=metadata||jsonb_build_object('operator_reset_subject_id',subject_principal_id),subject_principal_id=NULL,subject_kind=NULL WHERE subject_principal_id IN (SELECT id FROM reset_apps);
ALTER TABLE iam.authentication_events ENABLE TRIGGER authentication_events_append_only;
UPDATE iam.testing_environments SET created_by_application_id=NULL WHERE created_by_application_id IN (SELECT id FROM reset_apps);
CREATE TEMP TABLE reset_rows(rel oid NOT NULL,row_tid tid NOT NULL,PRIMARY KEY(rel,row_tid));
INSERT INTO reset_rows SELECT tableoid,ctid FROM iam.applications WHERE id IN (SELECT id FROM reset_apps);
INSERT INTO reset_rows SELECT tableoid,ctid FROM iam.principals WHERE id IN (SELECT id FROM reset_apps) AND kind='application';
CREATE TEMP TABLE reset_fks AS
SELECT c.oid,c.conrelid,c.confrelid,c.conname,
 (SELECT string_agg(format('child.%I=parent.%I',a.attname,b.attname),' AND ' ORDER BY x.ordinality)
  FROM unnest(c.conkey,c.confkey) WITH ORDINALITY x(child_att,parent_att,ordinality)
  JOIN pg_attribute a ON a.attrelid=c.conrelid AND a.attnum=x.child_att
  JOIN pg_attribute b ON b.attrelid=c.confrelid AND b.attnum=x.parent_att) AS join_sql
FROM pg_constraint c WHERE c.contype='f' AND c.conparentid=0;
DO $$ DECLARE r record; added bigint; pass_added bigint; BEGIN
 LOOP
  pass_added:=0;
  FOR r IN SELECT * FROM reset_fks LOOP
   EXECUTE format('INSERT INTO reset_rows SELECT child.tableoid,child.ctid FROM %s child JOIN %s parent ON %s JOIN reset_rows marked ON marked.rel=parent.tableoid AND marked.row_tid=parent.ctid ON CONFLICT DO NOTHING',r.conrelid::regclass,r.confrelid::regclass,r.join_sql);
   GET DIAGNOSTICS added=ROW_COUNT; pass_added:=pass_added+added;
  END LOOP;
  EXIT WHEN pass_added=0;
 END LOOP;
END $$;
SELECT json_build_object('apps',(SELECT json_agg(app_id ORDER BY app_id) FROM reset_apps),'rows',(SELECT json_object_agg(rel::regclass::text,n) FROM (SELECT rel,count(*) n FROM reset_rows GROUP BY rel) counted));
-- Fail closed if the closure reaches unrelated identities or non-app-owned data.
DO $$ BEGIN
 IF EXISTS(SELECT 1 FROM reset_rows WHERE rel::regclass::text NOT IN (
 'iam.principals','iam.applications','iam.application_secrets','iam.application_requested_scopes',
 'iam.application_approved_scopes','iam.application_webhook_endpoints','iam.application_webhook_signing_keys',
 'iam.application_obo_endpoints','iam.application_webhook_event_projections','iam.application_scope_requests',
 'iam.application_scope_messages','iam.application_testing_environments','iam.application_bundle_members',
 'iam.oauth_authorization_requests','iam.oauth_authorization_request_scopes','iam.oauth_authorization_codes',
 'iam.oauth_consent_grants','iam.oauth_consent_grant_scopes','iam.oauth_refresh_family_scopes',
 'iam.refresh_token_families','iam.refresh_tokens','iam.access_tokens','iam.access_token_scopes',
 'iam.obo_proofs','iam.outbox_event_recipients','iam.webhook_deliveries','iam.webhook_delivery_attempts'))
 THEN RAISE EXCEPTION 'Dependency closure reached an unreviewed table'; END IF;
 IF EXISTS(SELECT 1 FROM iam.principals p JOIN reset_rows r ON r.rel=p.tableoid AND r.row_tid=p.ctid WHERE p.kind<>'application' OR p.id NOT IN (SELECT id FROM reset_apps))
 THEN RAISE EXCEPTION 'Dependency closure reached an unrelated principal'; END IF;
END $$;
-- Foreign-key triggers remain enabled. Retry only unresolved dependency order.
ALTER TABLE iam.oauth_refresh_family_scopes DISABLE TRIGGER oauth_refresh_family_scopes_immutable;
CREATE TEMP TABLE reset_pending AS SELECT DISTINCT rel FROM reset_rows;
CREATE TEMP TABLE reset_deleted(rel oid PRIMARY KEY,n bigint);
DO $$ DECLARE r record; count_deleted bigint; progressed boolean; BEGIN
 WHILE EXISTS(SELECT 1 FROM reset_pending) LOOP
  progressed:=false;
  FOR r IN SELECT rel FROM reset_pending ORDER BY rel LOOP
   BEGIN
    EXECUTE format('DELETE FROM ONLY %s t USING reset_rows marked WHERE marked.rel=t.tableoid AND marked.row_tid=t.ctid',r.rel::regclass);
    GET DIAGNOSTICS count_deleted=ROW_COUNT;
    INSERT INTO reset_deleted VALUES(r.rel,count_deleted);
    DELETE FROM reset_pending WHERE rel=r.rel;
    progressed:=true;
   EXCEPTION WHEN foreign_key_violation THEN NULL;
   END;
  END LOOP;
  IF NOT progressed THEN RAISE EXCEPTION 'Dependency cycle or unreviewed external reference prevents deletion'; END IF;
 END LOOP;
END $$;
ALTER TABLE iam.oauth_refresh_family_scopes ENABLE TRIGGER oauth_refresh_family_scopes_immutable;
SET CONSTRAINTS ALL IMMEDIATE;
DO $$ DECLARE r record; value text; BEGIN
 IF (SELECT count(*) FROM iam.applications)<>1 OR NOT EXISTS(SELECT 1 FROM iam.applications WHERE app_id='tos>honeycomb') THEN RAISE EXCEPTION 'Final app set invalid'; END IF;
 FOR r IN SELECT * FROM reset_protected LOOP
  EXECUTE format('SELECT md5(COALESCE(jsonb_agg(to_jsonb(t) ORDER BY to_jsonb(t)::text),''[]''::jsonb)::text) FROM %s t WHERE to_jsonb(t)::text LIKE ''%%01a0a751-69c0-70b2-8456-96806349f999%%''',r.rel::regclass) INTO value;
  IF value<>r.fingerprint THEN RAISE EXCEPTION 'Honeycomb data changed in %',r.rel::regclass; END IF;
 END LOOP;
 FOR r IN SELECT * FROM reset_core LOOP
  EXECUTE format('SELECT md5(COALESCE(jsonb_agg(to_jsonb(t) ORDER BY id),''[]''::jsonb)::text) FROM %s t',r.rel::regclass) INTO value;
  IF value<>r.fingerprint THEN RAISE EXCEPTION 'Core identity data changed in %',r.rel::regclass; END IF;
 END LOOP;
 IF EXISTS(SELECT id FROM reset_testing EXCEPT SELECT id FROM iam.testing_environments) THEN RAISE EXCEPTION 'Testing environment removed'; END IF;
 IF EXISTS(SELECT 1 FROM pg_trigger WHERE tgname IN ('audit_events_append_only','authentication_events_append_only','oauth_refresh_family_scopes_immutable') AND tgenabled<>'O') THEN RAISE EXCEPTION 'History trigger was not restored'; END IF;
END $$;
INSERT INTO iam.audit_events(id,request_id,actor_principal_id,actor_kind,organization_id,action,target_type,metadata)
SELECT gen_random_uuid(),gen_random_uuid(),'01a06253-6cc9-7ff1-830c-7dc690c0e17f','carbon',id,'application.operator_reset','application',
 jsonb_build_object('source','explicit_user_database_reset','preserved','tos>honeycomb','removed',(SELECT jsonb_agg(app_id ORDER BY app_id) FROM reset_apps),'backup','silicon-iam-before-app-reset-20260916','history_preserved',true)
FROM iam.organizations WHERE org_id='tos';
SELECT json_build_object('remaining_apps',(SELECT json_agg(app_id) FROM iam.applications),'deleted_rows',(SELECT json_object_agg(rel::regclass::text,n) FROM reset_deleted),'history_preserved',true,'honeycomb_unchanged',true,'core_identities_unchanged',true);
-- Rehearsal defaults to rollback; commit requires a reviewed successful rehearsal.
ROLLBACK;
