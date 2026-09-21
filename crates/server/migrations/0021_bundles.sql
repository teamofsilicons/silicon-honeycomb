CREATE TABLE bundle_requests (
 operation_id TEXT PRIMARY KEY REFERENCES operations(id),
 bundle_id TEXT NOT NULL,
 body TEXT NOT NULL
);
CREATE INDEX bundle_requests_resource ON bundle_requests(bundle_id);
CREATE UNIQUE INDEX one_pending_bundle_configuration ON operations(plane,resource)
 WHERE kind='bundle.configure' AND state='pending';
