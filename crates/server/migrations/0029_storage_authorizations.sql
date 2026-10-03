CREATE TABLE storage_authorizations (
 id TEXT PRIMARY KEY, plane TEXT NOT NULL, generation INTEGER NOT NULL, actor TEXT NOT NULL, org_id TEXT NOT NULL,
 idempotency_key TEXT NOT NULL, request_hash TEXT NOT NULL, callback_state TEXT NOT NULL, redirect_uri TEXT NOT NULL,
 iam_request_id TEXT, consent_url TEXT, status TEXT NOT NULL DEFAULT 'pending', expires_at INTEGER NOT NULL,
 encrypted_request TEXT, exchange_key TEXT NOT NULL, created_at INTEGER NOT NULL,
 UNIQUE(plane,generation,actor,idempotency_key)
);
CREATE TABLE storage_grants (
 plane TEXT NOT NULL,generation INTEGER NOT NULL,actor TEXT NOT NULL,org_id TEXT NOT NULL,endpoint_id TEXT NOT NULL,
 encrypted_pair TEXT NOT NULL,expires_at INTEGER NOT NULL,refresh_key TEXT,lease_token TEXT,lease_until INTEGER,
 updated_at INTEGER NOT NULL,
 PRIMARY KEY(plane,generation,actor,org_id,endpoint_id)
);
