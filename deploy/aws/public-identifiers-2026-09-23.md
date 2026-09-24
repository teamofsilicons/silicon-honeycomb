# Public identifier deployment — 2026-09-23

## Final native distribution

Honeycomb0.4.0 is published both as the twelve GitHub native assets and as the six-target Honeycomb catalog archive. Catalog releasea95dd175-1b91-411b-a44c-859507994876 and publicationecbd9d5b-919f-4719-b305-b333ed636317 are published, archive SHA256574e060ace090a420a92d3dde577950158d148d745a9b8ad36ba7b685701889e. All native input archive hashes matched the public release before repacking. Core, client and CLI0.4.0 crates are published. The system Honeycomb command now reports0.4.0.

The documented offline installed-registry mapper was applied after stopping the corresponding maintenance workers and saving backups. Three local package homes changed4,4 and9 registry entries respectively; payload paths, aliases, credentials and package hashes were retained. Updated app packages preserve their aliases. Maintenance workers were restored. Temporary acceptance workers were removed. Ting's package remained0.1.5 and Starter remained0.2.1.
