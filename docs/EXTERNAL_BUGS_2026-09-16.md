# Stemcell compatibility fixes and verification — 2026-09-16

Scope: DM, Hook, Remind, Commit, Waveform, IAM, Briefcase, and Honeycomb.
Source: [Stemcell external dependency ledger](https://github.com/teamofsilicons/silicon-stemcell/blob/main/docs/EXTERNAL-BUGS.md).

## Changes

- Honeycomb accepts `HONEYCOMB_AUTO_UPDATE=0`, `false`, `off`, or `no` to disable scheduled CLI and package updates for the current invocation, including the daemon. Saved settings are preserved. Explicit self-update remains explicit, but command symlinks cannot be replaced; update those through their original package manager.
- Honeycomb records actual rejected release validation messages, scoped by application, production/testing context, and application revision. Publication rejection returns those messages in `error.details`; CLI and web error rendering preserve them. For example: `targets.windows-aarch64: required 64-bit target is missing`. A missing upload is identified separately from pending IAM activation. Old revisions do not supply current validation errors, successful uploads clear the saved rejection, and unauthorized users cannot read it. CLI uploads use the authenticated server validator; standalone `honeycomb validate` remains local.
- IAM maps the private-app cross-organization selection guard to HTTP 403 `private_application_organization_required`, with recovery instructions. It previously returned HTTP 500. Private apps still require their own organization; public apps still allow explicit grants for another current membership. The shared issuance path covers single, batch, and bundle login. Unexpected SQL failures remain internal.
- DM adds a regression running `dm iam --json` against the real current response envelope in a fresh home, without authentication. The underlying 0.7.0 parsing fix was already present.
- The Honeycomb CLI journey now verifies exact upload/publication diagnostics and the existing automatic-publication behavior after final approval.

Honeycomb's six required native targets remain required, as specified in its human-owned UNDERSTANDING.md. Supporting only four Unix targets does not meet that publication requirement. No Windows support is claimed or fabricated by these changes.

## Checks completed locally

| App | Verification |
| --- | --- |
| DM | Existing CLI/client suites: 16 passed; new envelope discovery regression: 1 passed; focused warning-denied Clippy. The ledger's fixed 0.7.0 source is an ancestor of the checkout. |
| Hook | CLI/client suites: 7 passed; update builder is a compatibility no-op. |
| Remind | CLI/client suites: 9 passed, including discovery, production/test session isolation, and Honeycomb-managed updates. |
| Commit | CLI/client suites: 16 passed, including logout and managed updates; backend IAM tests: 16 passed, including test credential pairing and no production fallback. Both historical fixes are present. |
| Waveform | CLI: 13 passed; client: 11 passed, including discovery and session isolation. |
| Briefcase | CLI/client checks with `honeycomb-managed`: 128 passed; 2 documentation examples ignored. Includes discovery and contract negotiation. The ledger's fixed 1.1.0 source is an ancestor of the checkout. Existing integration work was preserved. |
| IAM | Server library: 383 passed, 39 environment-dependent tests ignored in the normal run. The disposable PostgreSQL protocol suite was run explicitly and passed, including the new two-organization regression. Warning-denied server Clippy and formatting passed. OpenAPI, migration-security, and runtime-grant checks passed. |
| Honeycomb | Workspace: 142 passed. Focused publication tests and real CLI/HTTP/database journey passed. Warning-denied workspace Clippy, formatting, vendored IAM verification, reference/example checks, docs build and 25-page/1,412-link checks passed. |

The PostgreSQL test proves public second-organization selection succeeds, private own-organization selection succeeds, and private second-organization selection produces the exact 403 error. The publication tests compare errors from a real invalid archive with the later publication response, exercise the Rust client, reject outsiders, ignore stale revisions, and upload a corrected six-target archive.

## Delivery state

These are local source changes and tests. No commit, push, registry publication, or production deployment was performed. Production behavior has not been reverified with these fixes.

Honeycomb's server applies additive SQLite migration `0020_release_validation.sql` at startup; include it in the backend release. Ship the matching Honeycomb client/CLI to retain validation errors for rejected CLI uploads. IAM needs a backend release but no new database migration. Existing failed uploads cannot be reconstructed: re-upload the invalid archive after rollout to capture its exact validation result, or run `honeycomb validate` locally.
