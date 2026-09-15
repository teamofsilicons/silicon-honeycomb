ALTER TABLE operations ADD COLUMN request_json TEXT;
ALTER TABLE applications ADD COLUMN credential_version INTEGER NOT NULL DEFAULT 0;
