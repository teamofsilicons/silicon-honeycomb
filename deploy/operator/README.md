# Authorized IAM application reset — 2026-09-16

The user explicitly requested removal of all existing IAM applications except
Honeycomb, followed by a new Briefcase registration with fresh credentials. They
then explicitly requested clearing the old records from the database itself.
Normal IAM deletion retains immutable handles, so it cannot satisfy fresh reuse.

`iam-app-reset-plan.sql` is a one-time production maintenance transaction with
**ROLLBACK as its default**. It validates the exact ten reviewed `tos` handles and
the retained Honeycomb UUID before finding dependent rows through foreign keys.
The dependency allowlist prevents reaching unrelated identity tables. Foreign-key
checks stay enabled. Audit/authentication history retains the original UUIDs in
metadata while removing obsolete foreign-key references. Existing test environment
records remain, with removed creator-app links cleared. The separate testing
plane is not purged by this production transaction.

The transaction compares all IAM rows containing Honeycomb's UUID before and
after, and fingerprints Carbon, organization and membership records. It records
an operator audit entry and restores the narrow history triggers temporarily
paused for maintenance. It never changes runtime keys, service credentials, IAM
bootstrap files, or Honeycomb's application identity.

Backups before commit:

- Available encrypted RDS snapshot `silicon-iam-before-app-reset-20260916`.
- Host `/etc/silicon-iam/operator-app-reset/before-reset.dump`, PostgreSQL 17
  custom archive, 2,200,770 bytes; restore catalog validated (1,883 entries).
- Snapshot/host database: `silicon-iam-production`, region `us-east-1`.
- Host: `i-011c97da3d8b7ec74`; AWS profile: `silicon-production`.

Successful rollback rehearsal: SSM `23d13e13-0170-4b65-b1b4-073783719f21`.
It preserved Honeycomb and core identities, retained history, and removed only
reviewed application dependencies in 27 tables. The final execution tightens the
precondition from an app count to the exact sorted set of reviewed handles.

Run SQL using the migrator credential privately loaded from Secrets Manager on the
host; never print the password or put it in shell arguments. Replace only the final
`ROLLBACK;` with `COMMIT;` for the already-authorized, verified execution. This is
not a general application deletion endpoint or a recurring reset procedure.
