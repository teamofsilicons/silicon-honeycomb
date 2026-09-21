CREATE TABLE IF NOT EXISTS application_create_hash_contexts (
 operation_id TEXT PRIMARY KEY REFERENCES operations(id) ON DELETE CASCADE,
 application_id TEXT NOT NULL,
 legacy_hash_application_id TEXT NOT NULL CHECK(length(legacy_hash_application_id)=36)
);
