# Canonical application-owned environment repair

After the IAM canonical-ID cutover, application-authenticated environment management returned 503 because Honeycomb still required IAM's `application_id` to be a UUID. The retained SQLite ownership and link columns also held those old UUIDs, so changing only the validator would leave existing owners unable to manage their environments.

The backend now requires the protected IAM response's `application_id` to equal its validated canonical `app_id`, with the same organization and a valid organization resource UUID. It uses official IAM client 3.0.2. Legacy UUID identities are not accepted as authentication fallbacks. Attached applications still gain listing/import access only; environment keys and lifecycle operations require the original owner or existing separately authorized management paths.

The existing offline importer now converts these additional semantic references using only production `kind=application` export records:

- `environments.creator_application_id`, checked against `creator_app`;
- `environment_application_links.application_id`, checked against `app_id`;
- `application:<legacy UUID>` actor records, including lifecycle operations and activity inside testing worlds.

Application-create request hashes contain the application ID. The importer changes them only when the retained inputs reproduce the original digest exactly. If a world was edited or purged and the original input is unavailable, the old digest stays intact. Schema23 stores its legacy digest input in a private context bound to that exact operation and canonical application. Once canonical authentication and operation ownership succeed, an exact retry can reproduce the original hash; changed input still returns409. The old UUID is never used to authenticate an app, find an owner or select an environment. Other request hashes remain unchanged.

The identity columns remain TEXT. Migration0023 adds only the private per-operation digest-context table. The offline importer creates that exact table inside its import transaction; startup subsequently records the idempotent migration in the SQLx ledger. Before deployment, rehearse on a complete SQLite backup and compare row counts, environment IDs, root-key ciphertext/hashes, application credentials, and foreign-key/integrity checks. Then stop the backend, retain a quiesced backup, and run:

```sh
python3 scripts/import-iam-identities.py --database /path/to/honeycomb.sqlite --identity-map /protected/identity-mapping.json
```

The importer commits one transaction, rejects missing or contradictory ownership mappings, and is idempotent. A second run must report zero changes. Start the new backend only after the import succeeds. Do not restore an older database snapshot automatically after services resume.

Regression coverage uses the real SQLite schema and the real offline importer. It verifies that an existing canonical-authenticated app recovers the same environment, root key and create retry after backfill, while another application receives 404 for owner-only reads and key access. Separate tests cover revoked/mismatched identities, attached-app permissions, production-only mapping, preserved ciphertext/resource IDs, bound legacy digest contexts with exact replay and changed-input denial, transaction rollback, and repeat imports.

Validation before deployment: 30 server library tests, 6 application-environment integration tests, and 7 real-schema SQLite importer tests passed; strict server all-target Clippy passed. The integration test covers both an unchanged world and a world edited after creation, starting at schema22 and recording migration23 after offline import.

The copied live snapshot rehearsal (SSM `06c4aefc-0833-4d8b-8268-ae7ae55618c8`) converted 16 references/digests across 2 application-owned worlds and 3 application links. Both retained create digests could be verified and converted; no legacy hash contexts were required by this snapshot. Original table row counts, encrypted fields, and resource IDs were unchanged. SQLite integrity and foreign-key checks passed; a second import made zero changes, and no legacy owner/link IDs remained. The source snapshot was not modified. Rehearsal evidence is retained privately on the host under `/var/lib/silicon-honeycomb/backups/control-rehearsal-1789988153519291318`.

## Production deployment

Source `3a6351dad128366491854df73e90f4b28d29111b` was built for ARM64 and
published as immutable image:

```text
234951665042.dkr.ecr.us-east-2.amazonaws.com/silicon-honeycomb-backend@sha256:ad8bf554c2472be992fad0a57e9b32046fc2f148dd260acc2b57b79cfe3840a3
```

SSM `2397ab21-beb5-4985-a56b-3324330f667f` completed successfully. The
backend was paused for a final SQLite/configuration backup, uploaded before
conversion to the private versioned recovery bucket
`silicon-iam-recovery-234951665042-us-east-1`, key
`canonical-20260921/honeycomb-application-ownership-before.tar.gz`, version
`sRA6KQozyGLoThjpVfidqK7yasmfTeO3`. The verified SHA-256 is
`cfad7fc5d73237f64090f16f2098debbb273d0c952018d77d3d64cb4830c80cc`.
The temporary upload permission was removed after an independent checksum,
size, encryption, and version check.

The live import converted 16 references/digests and repeated as a no-op. All
existing business fingerprints, encrypted data, resource IDs, and container
configuration were preserved. SQLx recorded migration 23, foreign-key and
integrity checks passed, and the backend stayed healthy with the same container
start time and restart count during stabilization. The exact retained creates
were reconstructable; no private hash-context rows were needed in this database.
The previous container remains stopped and the private local backup is retained
at `/var/lib/silicon-honeycomb/backups/canonical-control-1789988409705926908`.
A normal saved-session `apps get tos>iam` resource read succeeded afterward.

The retained Commit application credentials then passed the affected live paths:
owner environment GET/list, the original root-key read, and the exact original
create body and idempotency key. Replay returned the same environment and original
operation; changing the body with that key returned 409. Inventory and environment
revision were unchanged by these probes.

Normal owner deletion of the agent-created Commit world
`036b5c48-0aaf-4bdf-83ca-8f1928b88d55` completed at revision 3 under operation
`ac0ea930-ba96-4bdb-b315-3c84893eda7c`. Commit, IAM, and Honeycomb each returned a
successful participant receipt for that operation, with no pending operation.
All seven probes using its old root key, application credentials, or retained
bearers with the deleted testing selector returned 401. No root-key ownership
bypass or direct database purge was used.
