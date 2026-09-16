-- App handles are reusable; immutable IAM identities alone confer ownership.
ALTER TABLE environments ADD COLUMN creator_application_id TEXT;
CREATE TABLE environment_application_links (
 environment_id TEXT NOT NULL REFERENCES environments(id),
 application_id TEXT NOT NULL, app_id TEXT NOT NULL,
 last_activity INTEGER NOT NULL, state TEXT NOT NULL DEFAULT 'pending',
 PRIMARY KEY(environment_id, application_id)
);
