-- Per-account read markers acknowledge only the exact request version viewed.
CREATE TABLE request_reads (
 plane TEXT NOT NULL, actor TEXT NOT NULL,
 request_id TEXT NOT NULL REFERENCES publication_requests(id) ON DELETE CASCADE,
 view TEXT NOT NULL CHECK(view IN ('sent','received')), provider TEXT NOT NULL DEFAULT '',
 activity_version TEXT NOT NULL CHECK(length(activity_version)=64), read_at INTEGER NOT NULL,
 PRIMARY KEY(plane,actor,request_id,view,provider)
);
-- Keep derived display timestamps outside immutable publication/hold evidence.
CREATE TABLE publication_activity (
 request_id TEXT PRIMARY KEY REFERENCES publication_requests(id) ON DELETE CASCADE,
 updated_at INTEGER NOT NULL
);
INSERT INTO publication_activity(request_id,updated_at)
SELECT id,MAX(created_at,
 COALESCE((SELECT MAX(created_at) FROM discussions d WHERE d.request_id=publication_requests.id),0),
 COALESCE((SELECT MAX(created_at) FROM decisions d WHERE d.request_id=publication_requests.id),0))
FROM publication_requests;
CREATE TRIGGER publication_request_created AFTER INSERT ON publication_requests
BEGIN INSERT INTO publication_activity(request_id,updated_at) VALUES(NEW.id,NEW.created_at); END;
CREATE TRIGGER publication_request_changed AFTER UPDATE OF state,error,plan_id,activation_operation ON publication_requests
WHEN NEW.state IS NOT OLD.state OR NEW.error IS NOT OLD.error OR NEW.plan_id IS NOT OLD.plan_id OR NEW.activation_operation IS NOT OLD.activation_operation
BEGIN UPDATE publication_activity SET updated_at=MAX(updated_at,unixepoch()) WHERE request_id=NEW.id; END;
CREATE TRIGGER publication_discussion_activity AFTER INSERT ON discussions
BEGIN UPDATE publication_activity SET updated_at=MAX(updated_at,NEW.created_at) WHERE request_id=NEW.request_id; END;
CREATE TRIGGER publication_decision_activity AFTER INSERT ON decisions
BEGIN UPDATE publication_activity SET updated_at=MAX(updated_at,NEW.created_at) WHERE request_id=NEW.request_id; END;
CREATE INDEX publication_activity_order ON publication_activity(updated_at DESC,request_id);
