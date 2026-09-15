CREATE TABLE api_contracts (
 version TEXT PRIMARY KEY,
 state TEXT NOT NULL CHECK(state IN ('active','deprecated','retired')),
 minimum_client TEXT NOT NULL,
 deprecated_at INTEGER,
 last_request_at INTEGER,
 retired_at INTEGER,
 CHECK(state='active' OR deprecated_at IS NOT NULL)
);
INSERT INTO api_contracts(version,state,minimum_client) VALUES('v1','active','0.1.0');
