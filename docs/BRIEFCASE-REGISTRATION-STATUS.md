# Briefcase registration status — 2026-09-16

**Created directly in production IAM and Honeycomb, public and approved.**

The user explicitly authorized direct database creation and public approval instead
of the normal registration request because the planned archive URL does not resolve.
No normal registration request was sent. This is an audited operator bootstrap,
not a successful archive upload or remote package validation.

## Applied configuration

- App: `tos>briefcase`, organization `tos`, name Silicon Briefcase.
- Fresh IAM UUID: `12f4471e-f078-45a0-99c1-1484437d0cf1`.
- IAM: public, verified, revision 3, configuration revision 1, credential version 1.
- Honeycomb: public, active, desired/effective configuration revision 1.
- All ten declared IAM scopes approved with `provider_approval` basis.
- Eleven OBO endpoints enabled. File creation, upload reservation and upload commit
  proofs last 3,600 seconds (60 minutes); the other endpoints retain 300 seconds.
- Fresh application and webhook credentials are saved in the protected local
  `~/.config/silicon/honeycomb/briefcase-fresh-20260916/credentials.json` (0600).
  Secrets are not included in Git or operator command logs.
- Honeycomb's existing IAM identity and connection were preserved.

[Full configuration and planned archive URLs](BRIEFCASE-REGISTRATION-PREVIEW.md).

## Release and remaining service work

Release `1.1.0` is cataloged with the validated local archive's exact SHA-256
`ccee3f85e257e61faa826f65dfff7b42d7964471cd3ea127d69b0f161281019e`
and size 20,711,083 bytes. Its storage reference explicitly records `pending_upload`
and `remote_archive_validated: false`.

Planned public bytes URL:
<https://backend.briefcase.teamofsilicons.com/api/v1/public/tos/apps/tos%3Ehoneycomb/public/tos--briefcase-1.1.0.tar.gz?view=attachment>

The archive has **not** been uploaded. Briefcase still needs its fresh credentials
configured, then the matching archive uploaded at the exact planned path with
public link access enabled. Public catalog visibility does not make the missing
archive downloadable. Honeycomb verifies downloaded bytes against the stored hash.

## Production verification

- IAM management GET confirms public/verified status, ten approved scopes, eleven
  endpoints and all proof lifetimes.
- Fresh application secret successfully authenticated to OAuth introspection:
  HTTP 200 with `active: false` for an intentionally nonexistent probe token.
- Honeycomb unauthenticated application and catalog reads return HTTP 200, public
  active Briefcase and latest version `1.1.0`.
- Background IAM reconciliation accepted revision 3 without an error and retained
  public visibility. IAM and Honeycomb publication gates are approved.
- No end-to-end archive download or running Briefcase login is claimed.

## Operator evidence

IAM transaction passed a rollback rehearsal before commit; database constraints
remained enabled. Backup: IAM host
`/etc/silicon-iam/operator-briefcase-20260916/before-create.dump`.

- IAM rehearsal SSM: `481435b3-09ad-4988-9420-26b3f9507475`.
- IAM backup/commit SSM: `057c4f61-558c-49db-96c8-f40d314efd42`.
- Honeycomb rehearsal SSM: `42a028fa-e347-4602-953f-c1b7969d0167`.
- Honeycomb commit SSM: `a3f1bf93-ef91-42ca-93ab-4a918607fe24`.
- Honeycomb verification SSM: `7aebc44b-40fb-4ecc-b1fd-156f3cf2c4d2`.
- SQLite backup: Honeycomb host
  `/var/lib/silicon-honeycomb/backups/briefcase-direct-20260916T032201Z/honeycomb.db`.
- Explicit operator publication operation: `4ceee2f5-6173-4b8f-8c14-29e5689d940b`.
- Publication: `a6231052-1406-4f31-97e2-4b4fa02b5fff`.

RDS master credentials were rotated through AWS to restore operator database
access after both stored credential versions were rejected. Runtime API database
credentials remained separate and continued authenticating.

The earlier user-authorized removal of old IAM applications is documented in
[the reset evidence](../deploy/operator/README.md). The earlier legacy adoption
implementation (`beb9c63`) was never used for Briefcase.
