CREATE TABLE reports (
 id TEXT PRIMARY KEY, plane TEXT NOT NULL, actor TEXT NOT NULL,
 idempotency_key TEXT NOT NULL, request_hash TEXT NOT NULL,
 message TEXT NOT NULL, pr TEXT, created_at INTEGER NOT NULL,
 UNIQUE(plane,actor,idempotency_key)
);
ALTER TABLE outbox ADD COLUMN next_attempt INTEGER NOT NULL DEFAULT 0;
ALTER TABLE outbox ADD COLUMN provider_id TEXT;
ALTER TABLE discussions ADD COLUMN idempotency_key TEXT;
ALTER TABLE discussions ADD COLUMN request_hash TEXT;
CREATE UNIQUE INDEX discussion_idempotency ON discussions(request_id,actor,idempotency_key);
