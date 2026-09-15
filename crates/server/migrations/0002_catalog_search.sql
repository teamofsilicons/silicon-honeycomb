CREATE VIRTUAL TABLE catalog_search USING fts5(
 plane UNINDEXED, app_id, variant UNINDEXED, name, description,
 tokenize='unicode61 remove_diacritics 2', prefix='2 3 4'
);
INSERT INTO catalog_search(plane,app_id,variant,name,description)
 SELECT plane,app_id,'desired',name,description FROM applications;
INSERT INTO catalog_search(plane,app_id,variant,name,description)
 SELECT plane,app_id,'effective',json_extract(effective_config,'$.name'),json_extract(effective_config,'$.description')
 FROM applications WHERE iam_revision>0;
CREATE TRIGGER catalog_insert AFTER INSERT ON applications BEGIN
 INSERT INTO catalog_search(plane,app_id,variant,name,description) VALUES(new.plane,new.app_id,'desired',new.name,new.description);
 INSERT INTO catalog_search(plane,app_id,variant,name,description)
 SELECT new.plane,new.app_id,'effective',json_extract(new.effective_config,'$.name'),json_extract(new.effective_config,'$.description') WHERE new.iam_revision>0;
END;
CREATE TRIGGER catalog_update AFTER UPDATE OF name,description,effective_config,iam_revision ON applications BEGIN
 DELETE FROM catalog_search WHERE plane=old.plane AND app_id=old.app_id;
 INSERT INTO catalog_search(plane,app_id,variant,name,description) VALUES(new.plane,new.app_id,'desired',new.name,new.description);
 INSERT INTO catalog_search(plane,app_id,variant,name,description)
 SELECT new.plane,new.app_id,'effective',json_extract(new.effective_config,'$.name'),json_extract(new.effective_config,'$.description') WHERE new.iam_revision>0;
END;
CREATE TRIGGER catalog_delete AFTER DELETE ON applications BEGIN
 DELETE FROM catalog_search WHERE plane=old.plane AND app_id=old.app_id;
END;
