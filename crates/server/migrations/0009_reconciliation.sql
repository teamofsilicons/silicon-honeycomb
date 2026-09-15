CREATE TABLE application_reconciliation (
 plane TEXT NOT NULL, app_id TEXT NOT NULL, target_revision INTEGER NOT NULL,
 state TEXT NOT NULL DEFAULT 'pending', attempts INTEGER NOT NULL DEFAULT 0,
 retry_at INTEGER NOT NULL DEFAULT 0, error TEXT, updated_at INTEGER NOT NULL,
 PRIMARY KEY(plane,app_id),
 FOREIGN KEY(plane,app_id) REFERENCES applications(plane,app_id) ON DELETE CASCADE
);
