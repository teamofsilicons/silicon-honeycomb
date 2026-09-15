# Implementation tracker

Source of requirements: human-owned `../UNDERSTANDING.md` (never edited by agents).

## Delivery order

1. Backend: live IAM authentication, organization authorization, safe packages,
   drafts and releases, durable configuration operations, publication reviews,
   private catalog filtering, metrics, testing lifecycle coordination.
2. Stateless Rust client and stateful CLI: SLT login, packaging, app administration,
   package installation/update/removal, testing selection, updater and shell installer.
3. Library and authenticated console on the requested domains.
4. Backend, CLI, installer and browser end-to-end verification.

## Integration findings

- Latest published `silicon-iam-client` verified with `cargo info`: 1.9.0.
- IAM supports application SLT exchange and live authorization snapshots.
- Current IAM source has no Honeycomb management service integration. Application
  administration there requires a direct IAM principal credential, which must not
  be substituted for Honeycomb's application token. The new integration requires
  service identity AND live acting-user authority, revisions and idempotency.
- Briefcase OBO supports recoverable uploads, reads, and critical
  `briefcase.link_access.update` for anyone-with-link sharing.
- Ecosystem testing lifecycle migration requires coordination with IAM and each
  application's protected lifecycle implementation. Never report all services
  ready solely because Honeycomb's local record exists.

Track external limitations explicitly. Test doubles are restricted to test code;
production has no fabricated-login or automatic-approval switch.

## Checkpoint: 2026-09-16

Implemented locally: core six-target package validation and safe extraction;
SQLite backend with official IAM runtime SDK authentication and current roles;
durable desired/accepted configuration; immutable Briefcase archive upload adapter;
private/public discovery, reviews, stars and download counts; shared drafts;
publication requests/discussions; encrypted environment keys and pending lifecycle
records; stateless Rust client; CLI and safe install/update/uninstall; verified
shell installer; Interface-styled Solid library and authenticated console with an
encrypted-session BFF.

Verification includes Rust archive/authorization/install tests, a CLI HTTP journey
against the isolated backend fixture, curl/bash installer checks, and desktop and
mobile browser journeys. These fixtures are not evidence of live IAM or Briefcase
integration. No repository, crates, binaries or websites have been published.

Still required before the full understanding is complete: provider/validator
review decisions and IAM activation/reconciliation; protected app secret rotation;
complete per-service testing lifecycle/import/migration/retention coordination;
Postmark report/notification delivery; public Briefcase download link persistence;
release distribution CI/deployment artifacts and actual live cross-service E2E.
