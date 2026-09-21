# IAM 3 consumer cutover

The server now uses the published IAM 3 Rust client and authenticates immutable canonical `public_id` values. Current IAM responses that include old extra UUID fields are accepted; missing or conflicting canonical identities fail closed. Recipient pagination accepts both historical UUID strings and new canonical Carbon-ID strings. Deserialization-only SDK aliases accept old job-role and recipient fields.

Before starting this backend, stop Honeycomb API and workers, snapshot the SQLite database and run:

```sh
python3 scripts/import-iam-identities.py --database /actual/honeycomb.sqlite --identity-map /private/identity-mapping.json
```

The export is a private pre-cutover IAM snapshot containing `production` and `testing` arrays of `legacy_id`, `public_id` and `testing_environment_id`. Keep it outside the repository. Root must refresh it immediately before the import.

The importer changes only explicit actor ownership/history columns and top-level notification-envelope actor metadata. It preserves resource IDs, operation keys, request hashes and user content. Testing identity mappings are selected by the actual environment. Missing mappings, conflicting canonical identities, duplicate rows or broken foreign keys abort the entire transaction. Exact retries are safe. It supports both the committed schema and later publication-intent tables if present.

Deploy this compatible backend before IAM 3 and retain it if IAM itself rolls back. Restoring an older Honeycomb backend requires restoring its pre-import database while all writers are stopped; an old backend would otherwise authenticate UUID actor strings against migrated canonical ownership. Do not restore an old snapshot after new production writes without reconciling those writes.

Tests: SDK authentication/current-and-canonical identity checks, testing credential recovery, management pagination, and the real-schema Python import test cover retained resource/hash bytes, environment isolation, rollback on incomplete exports and repeat imports.

## Reproducible backend baseline

The production backend deployed on September 17 already includes automatic publication, bundle management, release validation and the publication-intent retry queue (migrations 0018–0022). Those changes had been left uncommitted beside unrelated CLI, client and website work. The canonical-identity adapter commit alone is not a deployable replacement for that production image.

The following baseline commit records the existing server/core implementation, migrations, backend fixtures and deployment evidence together with the adapter. Build from the resulting commit, whose migration catalog includes 0022; do not build the earlier adapter-only commit. This does not add a new publication rollout, republish applications or alter the documented private/public approval rules. Unrelated CLI/client/UI drafts and the human-owned requirements remain outside this backend baseline.
