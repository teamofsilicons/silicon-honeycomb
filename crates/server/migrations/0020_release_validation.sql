-- Retain the latest rejected CLI upload for publication diagnostics.
CREATE TABLE release_validation_failures (
 plane TEXT NOT NULL, app_id TEXT NOT NULL, revision INTEGER NOT NULL,
 errors TEXT NOT NULL, created_at INTEGER NOT NULL,
 PRIMARY KEY(plane,app_id),
 FOREIGN KEY(plane,app_id) REFERENCES applications(plane,app_id) ON DELETE CASCADE
);
