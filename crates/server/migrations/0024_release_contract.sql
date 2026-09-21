-- Release-channel semantics use a new HTTP major; v1 retains its lifecycle and
-- minimum client policy so existing callers continue to receive the legacy shape.
INSERT INTO api_contracts(version,state,minimum_client) VALUES('v2','active','0.3.0');
