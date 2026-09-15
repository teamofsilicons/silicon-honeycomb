CREATE TABLE environment_app_activity (
 environment_id TEXT NOT NULL REFERENCES environments(id), app_id TEXT NOT NULL,
 last_activity INTEGER NOT NULL,
 PRIMARY KEY(environment_id,app_id)
);
INSERT INTO environment_app_activity(environment_id,app_id,last_activity)
 SELECT a.plane,a.app_id,COALESCE(i.last_activity,a.created_at)
 FROM applications a JOIN environments e ON e.id=a.plane
 LEFT JOIN environment_imports i ON i.environment_id=a.plane AND i.app_id=a.app_id;
CREATE TABLE environment_activity_events (
 environment_id TEXT NOT NULL REFERENCES environments(id), actor TEXT NOT NULL,
 idempotency_key TEXT NOT NULL, app_id TEXT NOT NULL, generation INTEGER NOT NULL,
 key_version INTEGER NOT NULL, received_at INTEGER NOT NULL,
 PRIMARY KEY(environment_id,actor,idempotency_key)
);
