-- Preserve the original requester while recording the manager whose current
-- authorization completes publication. Existing operations retain their actor.
ALTER TABLE publication_authorizations ADD COLUMN principal_id TEXT NOT NULL DEFAULT '';
UPDATE publication_authorizations SET principal_id=(SELECT requested_by FROM publication_requests WHERE id=request_id);
