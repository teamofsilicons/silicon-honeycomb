# Public identifier cutover

Applications now use a globally unique bare `app_id` (for example `briefcase`). Every configuration and IAM record carries its owning `org_id` separately. Carbon public IDs are `c:saket`, Silicon public IDs are `si:cos`, and public membership IDs retain the organization bracket: `c:saket[tos]` or `si:cos[tos]`. Bundle IDs remain organization-qualified, for example `tos>interface`; their member application IDs are bare. CLI release selectors remain `briefcase>test@2.1.0` and `briefcase@2.1.0`.

New application configuration uses `app_id` and `org_id`. The JSON input alias `local_app_id` is accepted during transition but serialized records always use `app_id`. Owning organization cannot change through an update. IAM snapshots must match the catalog's explicit organization. Archive storage and bundle membership authorization use recorded organization ownership.

## Inventory and mapping

Export the authoritative identity inventory from IAM before cutover. Resolve duplicate application handles and Silicon handles across organizations in IAM before applying the migration. Never pick an owner based on row order, silently merge registrations, or replace a UUID backing an identity. Include every referenced application, including external OBO providers and testing-plane records. Carbon/Silicon mappings must cover every retained actor in each plane. The Honeycomb file format is:

```json
{
  "applications": [
    {"legacy_id":"tos>briefcase","app_id":"briefcase","org_id":"tos"},
    {"legacy_id":"tos>honeycomb","app_id":"honeycomb","org_id":"tos"}
  ],
  "identities": [
    {"legacy_id":"saket","public_id":"c:saket"},
    {"legacy_id":"cos:tos","public_id":"si:cos"},
    {"legacy_id":"tester","public_id":"c:tester","testing_environment_id":"environment-uuid"}
  ]
}
```

The file is explicit authorization to preserve each identity under its new public name. Map retained UUID actor IDs too if an earlier public-identity import has not been applied. The program refuses missing actors, conflicting mappings, cross-organization application reassignment and colliding installed package keys.

## Backend sequence

1. Freeze registration/configuration/release writes. Drain pending operations, decisions, publication requests and outbox notifications, or preserve explicitly authorized unresolved work using the verified cutover hold below. A cancellation is valid only through its owning service's cancellation semantics. The migration refuses unheld pending work because replay bodies and hashes cannot be casually rewritten.
2. Stop Honeycomb API and all workers. Back up IAM and Honeycomb, then apply schema migrations through `0026_identifier_schema.sql` using the normal migration mechanism. Starting the new binary applies schema migrations but refuses to start workers while qualified application IDs remain.
3. Preview the exact IAM mapping:

   ```sh
   python3 scripts/migrate_identifier_schema.py --database data/honeycomb.db --map iam-id-map.json
   ```

4. Apply offline with a verified SQLite backup:

   ```sh
   python3 scripts/migrate_identifier_schema.py --database data/honeycomb.db --map iam-id-map.json --apply --backup-dir backups/identifier-cutover
   ```

5. Update deployment configuration: `HONEYCOMB_APP_ID=honeycomb`, `IAM_APPLICATION_ID=iam`, `BRIEFCASE_APP_ID=briefcase`, lifecycle participant IDs and OBO scopes such as `obo:briefcase:briefcase.files.read`. Keep application secrets, signing keys and unrelated UUID resource identifiers unchanged; migrate IAM text identity keys through IAM's own migration. Deploy IAM and all consumers of the new schema in the same maintenance window.
6. Expire old IAM/browser/CLI authentication sessions according to IAM's cutover rules and sign in again. Use fresh idempotency keys after cutover. Accepted/rejected historical operations retain their original request hashes and encrypted payloads; they are historical evidence, not new-schema mutation requests.
7. Verify catalog ownership in production and each testing plane; configure/update an existing app; test OBO approval, notification reconciliation, archive download/install, promotion, bundle membership, and retention operations. Confirm foreign keys and duplicate-free application IDs before enabling workers and reopening writes.

Migration uses one SQLite transaction, deferred foreign-key validation and exact typed field rewrites. It rebuilds search projections. It preserves opaque secrets, arbitrary `metadata`, messages/descriptions, binary archive bytes, checksums, versions and storage references. It creates `migrated_release_identities` entries for existing releases. New release APIs expose the exact old manifest ID only for these migration records. The CLI accepts that exact alias after verifying the catalog release checksum; an arbitrary other organization's legacy ID remains invalid. Promotion creates a new canonical manifest and version while preserving the source archive.

## Local installations

Stop the package maintenance worker and do not run install/update/uninstall during registry migration. Preview and apply each context's `installed.json` separately with the same approved mapping:

```sh
python3 scripts/migrate_identifier_schema.py --installed /absolute/context/installed.json --map iam-id-map.json
python3 scripts/migrate_identifier_schema.py --installed /absolute/context/installed.json --map iam-id-map.json --apply --backup-dir backups/installed-identifiers
```

Registry entries and channel keys change (`tos>briefcase>test` → `briefcase>test`). Physical package paths, command wrappers, symlinks, aliases, displaced-command backups and checksums stay unchanged so installed commands continue working. New installations use the bare ID directory. The launcher version detector understands both layouts. The registry rewrite is atomic and refuses collisions or disagreement between keys and recorded channel identity.

On failed verification keep writers stopped and restore the complete coordinated IAM/Honeycomb backups and old binaries/configuration. Restoring only one database while other services accept new-schema writes is not a supported rollback.

The migration preserves the deployed SQLx history exactly: `0023_application_create_hash_contexts.sql`, `0024_release_channels.sql` and `0025_release_contract.sql`. The new namespace schema is `0026_identifier_schema.sql`. Never reuse an applied migration number or change its checksum. App-owned environment actors and ownership links move to the bare application ID; accepted create operations keep their exact request hashes and retain an operation-bound legacy hash context, including any older UUID context, so matching retries still resolve to the same environment.

## Preserving unresolved work on a cutover hold

The coordinated September 23 cutover may preserve unresolved historical work on the explicit state `held_identifier_migration`. This is neither success nor cancellation. It authorizes no replay or delivery. Stop all writers and take the coordinated backup before creating an exact inventory manifest:

```sh
python3 scripts/hold_identifier_cutover.py --database data/honeycomb.db --manifest private/hold-manifest.json --capture
python3 scripts/hold_identifier_cutover.py --database data/honeycomb.db --manifest private/hold-manifest.json
python3 scripts/hold_identifier_cutover.py --database data/honeycomb.db --manifest private/hold-manifest.json --apply --stopped --backup-dir private/hold-backups
```

Capture is read-only and writes a new restricted hashes-only manifest. Preview rolls back all changes. Apply rechecks the complete inventory under a write transaction, verifies a SQLite backup, archives all original column values and hashes in `_identifier_cutover_holds`, and changes only each selected state. Encrypted `management_requests` belonging to held operations are archived too. Live leases, remaining test operations, activated publications, and publication authority/decision children are blockers requiring reconciliation; the tool cannot bypass them.

The ID mapper verifies each archived hash and current row before accepting held records. It changes only IAM-mapped actor/application routing columns; held notification payloads, publication snapshots, operation requests/results, request hashes, ciphertext and original receipt bytes remain exact. SQLite triggers block modification/deletion of that evidence and prevent unholding by a generic update. The server rejects configure/rotation recovery and publication-plan/activation retries with HTTP 409 `identifier_migration_hold`. Background dispatch only selects pending work and never sends held records.

Do not change held records back to pending. Reconciliation requires the original operation's authoritative IAM/provider outcome, a review of preserved request/hash context, the canonical identity mapping, and a separately reviewed transactional release procedure. A new publication request needs fresh current-revision authority and normal approvals; an old hold cannot be treated as approved. Notifications remain held until an operator determines how to deliver or retire the original event without changing its identity or implying prior delivery. Keep the hold manifest, exact original evidence, coordinated backups and migration receipts throughout this process.
