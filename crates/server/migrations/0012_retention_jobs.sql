CREATE TABLE retention_jobs (
 operation_id TEXT PRIMARY KEY REFERENCES operations(id),
 environment_id TEXT NOT NULL REFERENCES environments(id),
 next_attempt_at INTEGER NOT NULL DEFAULT 0,
 attempts INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE retirement_targets (
 operation_id TEXT NOT NULL REFERENCES operations(id),
 environment_id TEXT NOT NULL REFERENCES environments(id),
 app_id TEXT NOT NULL,
 PRIMARY KEY(operation_id,app_id)
);
CREATE INDEX retirement_targets_environment_app ON retirement_targets(environment_id,app_id);
CREATE INDEX retention_jobs_due ON retention_jobs(next_attempt_at);
