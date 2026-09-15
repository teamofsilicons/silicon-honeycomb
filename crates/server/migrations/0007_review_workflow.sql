ALTER TABLE publication_requests ADD COLUMN config_snapshot TEXT;
ALTER TABLE publication_requests ADD COLUMN plan_id TEXT;
ALTER TABLE publication_requests ADD COLUMN error TEXT;
CREATE TABLE review_gates (
 request_id TEXT NOT NULL REFERENCES publication_requests(id), provider TEXT NOT NULL,
 scopes TEXT NOT NULL, state TEXT NOT NULL DEFAULT 'pending', decision_id TEXT,
 PRIMARY KEY(request_id,provider)
);
CREATE UNIQUE INDEX decision_per_operation ON decisions(id);
UPDATE publication_requests SET state='awaiting_review_plan'
 WHERE plan_id IS NULL AND state='awaiting_scope_review';
