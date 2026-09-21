-- Existing releases belong to the production track. Environment plane remains independent.
CREATE TABLE releases_with_channels (
 plane TEXT NOT NULL, app_id TEXT NOT NULL, channel TEXT NOT NULL DEFAULT 'prod' CHECK(channel IN ('prod','dev')),
 version TEXT NOT NULL, sha256 TEXT NOT NULL, size INTEGER NOT NULL, storage_ref TEXT NOT NULL, created_at INTEGER NOT NULL,
 PRIMARY KEY(plane,app_id,channel,version), FOREIGN KEY(plane,app_id) REFERENCES applications(plane,app_id)
);
INSERT INTO releases_with_channels(plane,app_id,version,sha256,size,storage_ref,created_at)
 SELECT plane,app_id,version,sha256,size,storage_ref,created_at FROM releases;
DROP TABLE releases;
ALTER TABLE releases_with_channels RENAME TO releases;
CREATE TABLE publication_archives_with_channels (
 operation_id TEXT NOT NULL REFERENCES operations(id), channel TEXT NOT NULL DEFAULT 'prod' CHECK(channel IN ('prod','dev')),
 version TEXT NOT NULL, source_ref TEXT NOT NULL, published_ref TEXT, state TEXT NOT NULL DEFAULT 'pending', error TEXT,
 PRIMARY KEY(operation_id,channel,version)
);
INSERT INTO publication_archives_with_channels(operation_id,version,source_ref,published_ref,state,error)
 SELECT operation_id,version,source_ref,published_ref,state,error FROM publication_archives;
DROP TABLE publication_archives;
ALTER TABLE publication_archives_with_channels RENAME TO publication_archives;
CREATE TABLE release_validation_failures_with_channels (
 plane TEXT NOT NULL, app_id TEXT NOT NULL, channel TEXT NOT NULL DEFAULT 'prod' CHECK(channel IN ('prod','dev')),
 revision INTEGER NOT NULL, errors TEXT NOT NULL, created_at INTEGER NOT NULL,
 PRIMARY KEY(plane,app_id,channel), FOREIGN KEY(plane,app_id) REFERENCES applications(plane,app_id) ON DELETE CASCADE
);
INSERT INTO release_validation_failures_with_channels(plane,app_id,revision,errors,created_at)
 SELECT plane,app_id,revision,errors,created_at FROM release_validation_failures;
DROP TABLE release_validation_failures;
ALTER TABLE release_validation_failures_with_channels RENAME TO release_validation_failures;
-- Reserve an immutable channel/version before any external archive write.
CREATE TABLE release_reservations (
 plane TEXT NOT NULL, app_id TEXT NOT NULL, channel TEXT NOT NULL CHECK(channel IN ('prod','dev')),
 version TEXT NOT NULL, operation_id TEXT NOT NULL REFERENCES operations(id) ON DELETE CASCADE,
 PRIMARY KEY(plane,app_id,channel,version),
 FOREIGN KEY(plane,app_id) REFERENCES applications(plane,app_id) ON DELETE CASCADE
);
