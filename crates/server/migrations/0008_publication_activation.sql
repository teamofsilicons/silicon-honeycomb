ALTER TABLE publication_requests ADD COLUMN activation_operation TEXT;
CREATE TABLE publication_archives (
 operation_id TEXT NOT NULL REFERENCES operations(id), version TEXT NOT NULL,
 source_ref TEXT NOT NULL, published_ref TEXT, state TEXT NOT NULL DEFAULT 'pending', error TEXT,
 PRIMARY KEY(operation_id,version)
);
