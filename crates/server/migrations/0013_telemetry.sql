CREATE TABLE telemetry_events (
 id INTEGER PRIMARY KEY,
 plane TEXT NOT NULL REFERENCES environments(id),
 generation INTEGER NOT NULL,
 event TEXT NOT NULL,
 created_at INTEGER NOT NULL
);
CREATE INDEX telemetry_plane ON telemetry_events(plane,id);
