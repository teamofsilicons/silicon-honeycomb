-- Authorization to complete an explicitly requested publication, never a new
-- publication decision. Short-lived IAM credentials remain revocable and are
-- encrypted at rest; no refresh token or reviewer credential is retained.
CREATE TABLE publication_authorizations (
    request_id TEXT PRIMARY KEY REFERENCES publication_requests(id) ON DELETE CASCADE,
    encrypted_token TEXT NOT NULL,
    updated_at INTEGER NOT NULL,
    next_attempt_at INTEGER NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0
);
