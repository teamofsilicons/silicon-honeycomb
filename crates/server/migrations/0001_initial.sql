PRAGMA foreign_keys = ON;
CREATE TABLE IF NOT EXISTS applications (
 plane TEXT NOT NULL DEFAULT 'production', app_id TEXT NOT NULL, org_id TEXT NOT NULL,
 name TEXT NOT NULL, description TEXT NOT NULL, visibility TEXT NOT NULL DEFAULT 'private',
 state TEXT NOT NULL DEFAULT 'pending', revision INTEGER NOT NULL DEFAULT 1,
 iam_revision INTEGER NOT NULL DEFAULT 0, config TEXT NOT NULL, effective_config TEXT,
 webhook_secret TEXT NOT NULL, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
 PRIMARY KEY(plane, app_id)
);
CREATE INDEX IF NOT EXISTS apps_org ON applications(plane, org_id);
CREATE TABLE IF NOT EXISTS operations (
 id TEXT PRIMARY KEY, plane TEXT NOT NULL, actor TEXT NOT NULL, idempotency_key TEXT NOT NULL,
 kind TEXT NOT NULL, resource TEXT NOT NULL, request_hash TEXT NOT NULL, revision INTEGER NOT NULL,
 state TEXT NOT NULL DEFAULT 'pending', error TEXT, result TEXT, created_at INTEGER NOT NULL,
 UNIQUE(plane,actor,idempotency_key)
);
CREATE TABLE IF NOT EXISTS releases (
 plane TEXT NOT NULL, app_id TEXT NOT NULL, version TEXT NOT NULL, sha256 TEXT NOT NULL,
 size INTEGER NOT NULL, storage_ref TEXT NOT NULL, created_at INTEGER NOT NULL,
 PRIMARY KEY(plane,app_id,version), FOREIGN KEY(plane,app_id) REFERENCES applications(plane,app_id)
);
CREATE TABLE IF NOT EXISTS reviews (
 plane TEXT NOT NULL, app_id TEXT NOT NULL, actor TEXT NOT NULL,
 rating REAL NOT NULL CHECK(rating>=0 AND rating<=5), review TEXT NOT NULL, updated_at INTEGER NOT NULL,
 PRIMARY KEY(plane,app_id,actor), FOREIGN KEY(plane,app_id) REFERENCES applications(plane,app_id)
);
CREATE TABLE IF NOT EXISTS stars (
 plane TEXT NOT NULL, app_id TEXT NOT NULL, actor TEXT NOT NULL,
 PRIMARY KEY(plane,app_id,actor), FOREIGN KEY(plane,app_id) REFERENCES applications(plane,app_id)
);
CREATE TABLE IF NOT EXISTS downloads (
 plane TEXT NOT NULL, app_id TEXT NOT NULL, receipt TEXT NOT NULL, created_at INTEGER NOT NULL,
 PRIMARY KEY(plane,app_id,receipt)
);
CREATE TABLE IF NOT EXISTS publication_requests (
 id TEXT PRIMARY KEY, plane TEXT NOT NULL, app_id TEXT NOT NULL, revision INTEGER NOT NULL,
 state TEXT NOT NULL, requested_by TEXT NOT NULL, created_at INTEGER NOT NULL,
 UNIQUE(plane,app_id,revision)
);
CREATE TABLE IF NOT EXISTS discussions (
 id TEXT PRIMARY KEY, request_id TEXT NOT NULL REFERENCES publication_requests(id),
 actor TEXT NOT NULL, provider TEXT, message TEXT NOT NULL, created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS decisions (
 id TEXT PRIMARY KEY, request_id TEXT NOT NULL REFERENCES publication_requests(id),
 actor TEXT NOT NULL, provider TEXT NOT NULL, scopes TEXT NOT NULL, decision TEXT NOT NULL,
 reason TEXT NOT NULL, state TEXT NOT NULL DEFAULT 'pending', created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS drafts (
 plane TEXT NOT NULL, org_id TEXT NOT NULL, id TEXT NOT NULL, revision INTEGER NOT NULL,
 body TEXT NOT NULL, updated_at INTEGER NOT NULL, PRIMARY KEY(plane,org_id,id)
);
CREATE TABLE IF NOT EXISTS environments (
 id TEXT PRIMARY KEY, org_id TEXT NOT NULL, creator TEXT NOT NULL, creator_app TEXT,
 name TEXT NOT NULL, description TEXT NOT NULL, encrypted_key TEXT NOT NULL, key_hash TEXT NOT NULL UNIQUE,
 key_version INTEGER NOT NULL DEFAULT 1, generation INTEGER NOT NULL DEFAULT 1,
 revision INTEGER NOT NULL DEFAULT 1, state TEXT NOT NULL DEFAULT 'provisioning',
 idle_days INTEGER NOT NULL DEFAULT 30, created_at INTEGER NOT NULL, last_activity INTEGER NOT NULL,
 deleted_at INTEGER, purge_after INTEGER
);
CREATE TABLE IF NOT EXISTS environment_services (
 environment_id TEXT NOT NULL REFERENCES environments(id), app_id TEXT NOT NULL,
 source_revision INTEGER NOT NULL, snapshot TEXT NOT NULL, state TEXT NOT NULL,
 operation_id TEXT NOT NULL, error TEXT, generation INTEGER NOT NULL,
 PRIMARY KEY(environment_id,app_id)
);
CREATE TABLE IF NOT EXISTS webhook_events (
 plane TEXT NOT NULL, id TEXT NOT NULL, resource TEXT NOT NULL, revision INTEGER NOT NULL,
 created_at INTEGER NOT NULL, PRIMARY KEY(plane,id)
);
CREATE TABLE IF NOT EXISTS audit (
 id TEXT PRIMARY KEY, plane TEXT NOT NULL, actor TEXT NOT NULL, action TEXT NOT NULL,
 resource TEXT NOT NULL, created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS outbox (
 id TEXT PRIMARY KEY, plane TEXT NOT NULL, event_key TEXT NOT NULL UNIQUE,
 kind TEXT NOT NULL, payload TEXT NOT NULL, state TEXT NOT NULL DEFAULT 'pending',
 attempts INTEGER NOT NULL DEFAULT 0, error TEXT, created_at INTEGER NOT NULL
);
