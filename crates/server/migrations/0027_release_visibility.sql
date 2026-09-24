-- Release visibility is independent of application visibility. Keep already
-- distributed archives public; new uploads default to private until accepted.
ALTER TABLE releases ADD COLUMN visibility TEXT NOT NULL DEFAULT 'private' CHECK(visibility IN ('private','public'));
ALTER TABLE releases ADD COLUMN configuration_revision INTEGER NOT NULL DEFAULT 0;
ALTER TABLE releases ADD COLUMN permission_approval_required INTEGER NOT NULL DEFAULT 0 CHECK(permission_approval_required IN (0,1));
UPDATE releases SET
 visibility=(SELECT visibility FROM applications a WHERE a.plane=releases.plane AND a.app_id=releases.app_id),
 configuration_revision=(SELECT CASE WHEN visibility='public' THEN effective_revision ELSE revision END FROM applications a WHERE a.plane=releases.plane AND a.app_id=releases.app_id);
CREATE INDEX releases_visibility ON releases(plane,app_id,channel,visibility);
