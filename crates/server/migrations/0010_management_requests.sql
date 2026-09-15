CREATE TABLE management_requests (
 operation_id TEXT PRIMARY KEY REFERENCES operations(id) ON DELETE CASCADE,
 kind TEXT NOT NULL, app_id TEXT NOT NULL, encrypted_body TEXT NOT NULL,
 created_at INTEGER NOT NULL
);
