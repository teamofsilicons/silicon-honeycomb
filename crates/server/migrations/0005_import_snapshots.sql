ALTER TABLE applications ADD COLUMN effective_revision INTEGER NOT NULL DEFAULT 0;
UPDATE applications SET effective_revision=COALESCE(
 (SELECT MAX(o.revision) FROM operations o WHERE o.plane=applications.plane AND o.resource=applications.app_id AND o.kind='configure' AND o.state='accepted'),
 CASE WHEN iam_revision>0 THEN revision ELSE 0 END);
ALTER TABLE environment_services ADD COLUMN receipt TEXT;
CREATE TABLE environment_imports (
 environment_id TEXT NOT NULL REFERENCES environments(id), app_id TEXT NOT NULL,
 source_revision INTEGER NOT NULL, snapshot TEXT NOT NULL, last_activity INTEGER NOT NULL,
 PRIMARY KEY(environment_id,app_id)
);
