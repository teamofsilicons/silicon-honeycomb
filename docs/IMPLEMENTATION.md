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

Still required before the full understanding is complete: IAM activation/reconciliation and live provider/validator integration; live protected app secret rotation;
complete per-service testing lifecycle/import/migration/retention coordination;
live Postmark delivery and IAM recipient discovery; actual release publication/deployment and live cross-service E2E.

Added since the frontend checkpoint: persisted public Briefcase download paths;
weighted FTS5 plus trigram search; prebuilt binary updater; six-platform release
candidate workflow; production container and HTTPS routing artifacts. Cargo source
installation and both production container modes have been smoke-tested locally.

Bug reports now queue durable Postmark notifications, accept optional PR links, and
return a report receipt. Release/publication discussions queue owner/admin notices.
Test-plane mail is captured. Uncertain delivery receipts are held for reconciliation
instead of automatically duplicating messages. No real emails have been sent.

Shared lifecycle operations now persist per-service receipts, use renewable coordinator
leases, retry incomplete services, and gate readiness on matching revisions, generations
and key versions. Clean, key rotation, soft delete, restore and post-retention purge are
covered by isolated backend tests and exposed in the console and CLI. Production
service transport remains unavailable until the protected IAM/service APIs exist.

Checkpoint checks: full Rust workspace tests and strict Clippy pass; CLI and curl/bash
installer journeys pass; web production build and all 14 desktop/mobile browser
journeys pass, including single-use refresh-token concurrency. Postmark delivery is
verified against a local HTTP double only.

Recursive imports now walk the entire external-scope dependency graph with cycle
detection, production visibility checks, source configuration/release pins and
explicit refresh. IAM must confirm each isolated import; all service receipts are
required before Honeycomb installs its test catalog records. Existing ready apps
remain available while an additional import is pending. CLI and console expose
imports and their progress. Imported production webhook/app secrets are never copied.
Selected source releases are recorded; copying archives into test storage still
requires the protected cross-service import implementation.

Import checkpoint verification: all 19 backend API tests, workspace Rust tests,
strict Clippy, CLI import journey, production web build and all 16 desktop/mobile
browser journeys pass. Coverage includes a 40-node cyclic graph and root-key-only
private-access denial. No live management integration was used.

App-secret rotations now require current admin authority and the accepted config
revision, keep stable operation IDs, pass transient IAM step-up evidence, and never
persist returned secrets. Explicit operation recovery can reconcile a lost rotation
response. Creation replay can recover IAM's short-lived secret result. The CLI and
console expose rotations, retries and recovery. Hosted IAM step-up and actual
credential rotation remain external integration dependencies.

Secret checkpoint verification: full Rust workspace tests (21 backend API cases),
strict Clippy, CLI rotation/pending checks, production web build, and all 18
desktop/mobile browser journeys pass. Browser retries preserve the original
idempotency key. Existing public distribution and live IAM limitations still apply.

Provider review workflow now includes IAM-derived review plans, provider/admin and
explicit validator authority, scoped discussions, denial reasons, durable decisions,
recipient-role notification queues, stale-revision rejection and provider-before-
validator sequencing. Public apps can request review of new scopes while retaining
older effective access. CLI and console expose the same inbox and decision flow.
All approvals stop at awaiting_activation; final IAM public activation and archive
sharing reconciliation are the next implementation step.

Review checkpoint verification: 24 backend API tests, full Rust workspace tests,
strict Clippy, CLI review/decision journeys, production web build and all 20
browser journeys pass. Review authority and receipt tests use explicit local doubles;
they do not claim that IAM's protected management API exists yet.
