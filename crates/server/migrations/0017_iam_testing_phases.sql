-- One Honeycomb lifecycle operation may contain multiple independently replayable IAM phases.
CREATE TABLE iam_testing_phases (
 parent_operation TEXT NOT NULL REFERENCES operations(id) ON DELETE CASCADE,
 phase TEXT NOT NULL, operation_id TEXT NOT NULL UNIQUE, environment_id TEXT NOT NULL,
 encrypted_body TEXT NOT NULL, receipt TEXT, created_at INTEGER NOT NULL,
 PRIMARY KEY(parent_operation, phase)
);
CREATE TABLE iam_testing_state (
 environment_id TEXT PRIMARY KEY REFERENCES environments(id),
 iam_revision INTEGER NOT NULL, generation INTEGER NOT NULL,
 key_version INTEGER NOT NULL, state TEXT NOT NULL
);
ALTER TABLE environment_services ADD COLUMN finalization_required INTEGER NOT NULL DEFAULT 0;
