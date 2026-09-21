-- Pending default-public requests use only a revocable manager access token.
-- No refresh token is retained. Expired authorization requires a manager retry.
CREATE TABLE publication_intents (
    plane TEXT NOT NULL,
    app_id TEXT NOT NULL,
    revision INTEGER NOT NULL,
    principal_id TEXT NOT NULL,
    encrypted_token TEXT NOT NULL,
    updated_at INTEGER NOT NULL,
    next_attempt_at INTEGER NOT NULL,
    error TEXT,
    PRIMARY KEY(plane,app_id),
    FOREIGN KEY(plane,app_id) REFERENCES applications(plane,app_id) ON DELETE CASCADE
);
