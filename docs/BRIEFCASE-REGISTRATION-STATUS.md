# Briefcase registration status — 2026-09-16

The operator preview confirmed that IAM already contains `tos>briefcase`, immutable
ID `01a070db-89b4-7542-83f1-4fad5cbce625`, IAM revision 9, configuration revision 0,
and credential version 1. No Honeycomb app existed at the start of this work.

**No adoption or application creation was executed.** The user clarified that the
new Briefcase app must not link to the existing IAM app. The import plan is
cancelled. Clarification is pending on whether the new app should be Honeycomb-only
or should receive a separate, fresh IAM identity. Do not apply the legacy import
command to Briefcase without a new instruction superseding this decision.

## Completed infrastructure work

- `beb9c63` adds compatibility for explicitly imported revision-zero IAM records,
  a privileged import tool with backup/audit/identity preconditions, and tests.
- Local server tests, strict workspace Clippy, formatting, dependency verification,
  and Python import tests passed.
- AWS deployment `482946e1-e129-4fbc-9012-ba0ec253501c` succeeded on
  `i-06986627793021fb2`. Backend image:
  `sha256:30f17b8ec535e7560cea6897792b67feb68356d5d01a2cea8e12457846e0ee42`.
- Preview/backup SSM command `22896753-2023-473c-a2bc-11a89fde874f` succeeded.
  Consistent backup: `/var/lib/silicon-honeycomb/backups/20260916T024022Z`.
- The preview staged the operator script and catalog metadata on the host but did
  not write an application record. No IAM mutation or credential rotation occurred.

The current registration/download/publication implementation expects an IAM-backed
application. Supporting an application with no IAM identity requires explicit
product behavior, not an invented accepted IAM revision in the database.

The user has prepared a Briefcase `.tar.gz`; its local path has not been supplied
and its contents have not been validated in this task.
