-- Preserve already-sent IAM review links without exposing their private threads.
-- Only the explicit, audited operator imports a mapping after IAM accepts a plan.
CREATE TABLE legacy_review_handovers (
 source_sha256 TEXT PRIMARY KEY CHECK(length(source_sha256)=64),
 plane TEXT NOT NULL,
 app_id TEXT NOT NULL,
 application_id TEXT NOT NULL,
 request_id TEXT NOT NULL UNIQUE REFERENCES publication_requests(id),
 state TEXT NOT NULL CHECK(state IN ('prepared','complete')),
 prepared_at INTEGER NOT NULL,
 completed_at INTEGER,
 source_requests TEXT NOT NULL,
 UNIQUE(plane,app_id)
);
CREATE TABLE legacy_review_requests (
 plane TEXT NOT NULL,
 legacy_request_id TEXT NOT NULL,
 request_id TEXT NOT NULL REFERENCES publication_requests(id),
 provider TEXT NOT NULL,
 source_sha256 TEXT NOT NULL CHECK(length(source_sha256)=64),
 imported_at INTEGER NOT NULL,
 PRIMARY KEY(plane,legacy_request_id),
 FOREIGN KEY(request_id,provider) REFERENCES review_gates(request_id,provider)
);
