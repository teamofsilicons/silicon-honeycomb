-- Mapping is populated by the offline identifier migration. Archive bytes, hashes,
-- versions and storage references remain immutable.
CREATE TABLE migrated_release_identities (
 plane TEXT NOT NULL, app_id TEXT NOT NULL, channel TEXT NOT NULL, version TEXT NOT NULL,
 legacy_app_id TEXT NOT NULL,
 PRIMARY KEY(plane,app_id,channel,version),
 FOREIGN KEY(plane,app_id,channel,version) REFERENCES releases(plane,app_id,channel,version) ON DELETE CASCADE
);

-- Retain the exact historical digest input for accepted app-owned environment
-- retries. Older inputs may be either a UUID or the former org>app public ID.
ALTER TABLE application_create_hash_contexts RENAME TO application_create_hash_contexts_previous;
CREATE TABLE application_create_hash_contexts (
 operation_id TEXT PRIMARY KEY REFERENCES operations(id) ON DELETE CASCADE,
 application_id TEXT NOT NULL,
 legacy_hash_application_id TEXT NOT NULL CHECK(length(legacy_hash_application_id) BETWEEN 1 AND 131)
);
INSERT INTO application_create_hash_contexts SELECT * FROM application_create_hash_contexts_previous;
DROP TABLE application_create_hash_contexts_previous;
