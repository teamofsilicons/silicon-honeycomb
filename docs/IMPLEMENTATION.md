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

- Registry SDK: 1.9.0. Local official IAM commit `7abd575` adds SDK 1.10.0;
  its unchanged source is pinned in vendor until publication. See IAM-CONTRACT-REVIEW.md.
- IAM supports application SLT exchange and live authorization snapshots.
- IAM now has a dedicated Honeycomb service integration with service identity
  AND live acting-user authority, revisions and idempotency. Supported production
  configuration/rotation/reads are wired; review, lifecycle and adoption contract
  differences remain explicit in IAM-CONTRACT-REVIEW.md.
- Briefcase OBO supports recoverable uploads, reads, and critical
  `briefcase.link_access.update` for anyone-with-link sharing.
- Ecosystem testing lifecycle migration requires coordination with IAM and each
  application's protected lifecycle implementation. Never report all services
  ready solely because Honeycomb's local record exists.

Track external limitations explicitly. Test doubles are restricted to test code;
production has no fabricated-login or automatic-approval switch.

## Checkpoint: 2026-09-16

API governance: unversioned contract discovery, explicit client/version negotiation,
legacy v1 compatibility and persisted deprecation/last-request timestamps now enforce
the seven-day idle sunset rule. Active contracts remain active; only an explicit
operator deprecation starts retirement. The real Rust client consumes the backend
contract in an HTTP test, alongside CLI and browser journeys. Version policy and
the consumer matrix are documented in [API-CONTRACTS.md](API-CONTRACTS.md).
All 44 API cases, workspace tests, strict Clippy, CLI journey, web build and 36
browser cases pass. Source and GitHub `v0.1.0` are now public; all three crates are published. All six
native release jobs and both public installer paths passed.
The user selected MIT for Honeycomb and approved publishing the vendored IAM SDK.
The root and crates carry MIT license text; the SDK's original notice remains intact.

Missed-notification recovery: the reconciliation worker durably schedules known
production applications for an authoritative IAM read every five minutes, with
bounded batches and preserved newer notification targets/backoff. This covers
accepted applications already in Honeycomb; legacy inventory/adoption and shared
testing lifecycle integration remain separate. A regression test verifies missed
revocation recovery and protects pending newer revisions.
Full workspace tests (42 backend API cases) and strict Clippy pass. Current backend
and web production images build, and disposable container smoke checks pass without
live IAM calls.

Latest telemetry checkpoint: official Space Station SDK export on the backend,
authenticated CLI/daemon/web diagnostics, persisted opt-out propagated through API
and IAM session requests, and generation-fenced local testing records. The dedicated
`tos/siliconhoneycomb` table is created and a synthetic record was acknowledged and
verified in its browser UI. No key is committed or shipped to clients. Full Rust
tests, strict Clippy, CLI HTTP journey, production web build and all 36 desktop/mobile
browser cases pass. See [TELEMETRY.md](TELEMETRY.md). Deployment secrets still need
provisioning; the live synthetic check does not imply the product is deployed.

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

Still required before the full understanding is complete: live IAM reconciliation/activation/provider/validator integration; live protected app secret rotation;
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
Approvals lead to a separate recoverable activation operation.

Review checkpoint verification: 24 backend API tests, full Rust workspace tests,
strict Clippy, CLI review/decision journeys, production web build and all 20
browser journeys pass. Review authority and receipt tests use explicit local doubles;
they do not claim that IAM's protected management API exists yet.

Public activation now persists IAM acceptance and per-release archive sharing,
retries only unfinished sharing, and verifies IAM's current public revision before
exposing the catalog entry. Config edits, rotations and release mutations are
serialized against activation. A public app's pending configuration can complete
through its approved publication. Operation leases prevent stale workers from
committing results. CLI and console expose activation and recovery progress.

Activation checkpoint verification: full Rust workspace tests (26 backend API
cases), strict Clippy, CLI approval/activation/anonymous installation and execution,
production web build, and all 22 desktop/mobile browser journeys pass. Browser
coverage includes archive upload, independent validator approval, console activation
and anonymous library discovery. These use explicit IAM/storage doubles; live
management activation, real archive sharing and deployment remain unverified.

Signed management notifications now queue durable accepted-state reconciliation.
Newer events suspend anonymous visibility until the protected read confirms current
state and a completed Honeycomb publication. Duplicate/stale events cannot restore
access; disabled apps cannot be downloaded. A retry worker and CLI/console refresh
expose pending reads. SDK signature verification and test key/generation isolation
remain mandatory; the SDK test-metadata incompatibility is in IAM-HANDOFF.md.

Reconciliation checkpoint: 28 backend API cases pass, including signed tampering,
stale/repeated events, revocation, publication gating, test-plane isolation and old
generation rejection. Full workspace tests, strict Clippy, CLI journey, web build
and 22 browser journeys passed; the four affected browser cases were rerun after
adding synchronization UI and isolated CLI homes. Live management reads still
require the future IAM service API.

A subsequent local IAM commit was discovered during contract verification. The
backend now pins its official 1.10 SDK source and supports real production
configuration, secret rotation/replay, accepted-state reads and separately signed
management notifications when provisioned. Encrypted immutable wire requests keep
SDK mutation retries exact. Test configuration, publication review/activation and
shared lifecycle adapters remain incomplete rather than fabricating receipts.

SDK checkpoint verification: all workspace tests (29 API cases plus three new
management SDK wire cases), strict Clippy, CLI/curl-bash installer journeys,
production web build and all 22 browser journeys pass. The source provenance hash
check passes. No live IAM credential, deployment, remote commit or publication was
used; see the current contract review before provisioning IAM's writer cutover.

The console now loads scoped IAM permission descriptions and eligibility, includes
public-review guidance, and keeps picker changes synchronized with editable scope
JSON/drafts. Private provider discovery requires current membership; catalog access
requires current organization administration. Permission discovery uses the official
1.10 catalog API. Thirty backend API cases and 24 desktop/mobile browser cases pass
(the two new browser cases were rerun after a locator-only correction).

See LOCAL-HANDOFF.md for the current local deliverable and the remaining full-product
work. Automatic retirement/migration, app-owned test setup, live review/lifecycle
adapters, telemetry, logo uploads and public rollout are not marked complete.


### Webhook management continuation

Connected IAM's pending endpoint read, exact destination approval and signing-key
rotation to durable Honeycomb operations, stateless Rust client, CLI and console.
Every mutation requires current organization administration, the current accepted
configuration, IAM revision and verified-channel step-up where enforced by IAM.
The adapter uses the pinned official SDK. Retry keeps the original endpoint/secret,
operation ID and wire body while permitting fresh transient verification evidence.
Only encrypted signing material is retained; operations and audit expose no secret.
An accepted rotation updates Honeycomb's retained signing secret for future config
writes. Competing configuration, application-secret and publication operations are
blocked while the webhook change is pending. The console also accepts step-up
assertions for its existing application-secret rotation/retry flow.

Checks cover exact SDK service/actor headers and bodies, stale endpoint/revision
rejection, non-admin denial, lost-response replay, encrypted persistence and real
CLI/browser paths using explicit fixtures. Live IAM acceptance still requires the
provisioned service integration and is not claimed by local tests.

Verification completed: workspace Rust tests (31 backend API cases and four official
management SDK wire cases), strict Clippy, CLI HTTP journey, web production build
and all 26 browser cases pass. The new browser test now creates its own application
to avoid changing fixtures shared with secret-rotation tests. The application-secret
step-up fields also passed a subsequent desktop/mobile check proving that fresh
proofs preserve the original retry key and clear from the form after submission.
The webhook screen was visually checked at the mobile viewport.


### Activity and retention controls

Added generation/key-version-bound activity reports using server timestamps,
actor-scoped idempotency and current environment authority. A replay does not
refresh the idle timer. Reports only affect an existing active app in that ready
environment; clean/purge remove activity and dedupe data. Import completion starts
the imported application's activity timer.

Added revisioned environment idle-day controls and read-only retention plans.
Plans use accepted production app retention, preserve transitive dependencies of
active applications, handle cycles and extend the shared deadline for longer app
retention. Backend, stateless client, CLI and console expose these controls. The
application input now actually includes testing_idle_days (30 by default), fixing
a missing field in ordinary IAM configuration requests; old saved configurations
also get the 30-day fallback in the SDK adapter.

Automatic per-app retirement and environment cleanup are still unfinished; this
checkpoint provides tracking, controls and the dependency-aware eligibility plan.
It does not claim to have enabled the automatic lifecycle worker.


Validation: all workspace Rust tests (34 backend API cases), strict Clippy, the real
CLI HTTP journey with retention/activity commands and the production web build pass.
The browser suite passed 27 of 28 cases; the new retention test was checking mobile
navigation before authenticated rendering. After waiting for the dashboard, both
its desktop and mobile variants passed. The retention screen was visually inspected.


### Automatic retention coordination

Added a startup worker that checks retention every minute and commits due decisions
with the environment revision. Durable jobs use exponential retry backoff (one
minute to one hour) and the existing renewable coordinator lease. App retirement
keeps the shared environment ready, blocks only selected apps' access/activity and
new test configuration while cleanup is pending, and preserves active data and
transitive dependencies. Every participant, including storage services, receives
the exact retired-app list and must acknowledge that list with the operation,
environment revision, generation and key version. IAM must acknowledge before
local retirement starts. Completed steps are skipped on retry.

Soft deletion uses the existing disable-acknowledgment barrier before starting the
30-day recovery window; purge is queued only at expiry. Retirement/deletion/purge
do not refresh activity. Local cleanup removes only selected test-plane data and
retired import snapshots, while production remains untouched. The CLI's retention
view and console show the current automatic job, targets and retry timing.

The service transport is explicitly separate from user-authorized lifecycle calls;
no user token is fabricated or retained. IAM's current adapter does not yet support
this automatic cross-service contract, so its jobs remain pending. This implements
the scheduler and coordinator, not live IAM/Briefcase acceptance.

Configured the user-provided GitHub SSH URL as origin. Its branch list is empty;
no commits have been pushed.


Validation: workspace Rust tests (37 backend API cases), strict Clippy, production
web build, real CLI HTTP journey and all 30 browser cases pass. The final database
migration also passed the 37-case backend suite. Tests exercise exact retirement
receipts, IAM-first sequencing, failed storage retry, active/dependency/production
preservation, disabled-access barriers, recovery deadlines and retry backoff.

### Cargo distribution verification

Verified all three publishable crates with `cargo package --workspace --exclude
silicon-honeycomb-server --locked`. Cargo 1.98 builds their packaged contents
against its temporary registry before any public publication. Added this check
and isolated Cargo source installation to CI. The six-platform release workflow
now also checks source installation, including the Windows executable suffix,
and explicitly selects the required toolchain for Python-invoked Cargo commands.

Corrected the deployment notes to reflect the configured SSH remote and distinguish
local package verification from crates.io installation. Local Cargo installation
passed again, including version, unauthenticated status and updater opt-out.
No Git push, public release, crate publication or deployment was performed.

### Application logo workflow

Added logo URL/file controls to application creation and editing, plus
`honeycomb apps upload-logo <org> <file>` and `Client::upload_logo`. The upload API
is `POST /api/v1/organizations/{org}/logos`, with the current IAM actor, an
idempotency key and raw image bytes. Current organization admin authority is
required before validation or replay. PNG/JPEG/WebP input is capped at 2 MiB and
2048 × 2048; decoding has allocation limits, and re-encoding strips metadata and
trailing content. Invalid input never creates a pending operation.

The Briefcase adapter reserves, transfers, commits and shares the normalized PNG
in Honeycomb's public app folder. The selected organization is bound into IAM's
OBO exchange; test authentication and the returned test-plane link are preserved.
Uploaded logos use publicly viewable URLs, explicitly disclosed in the form and
CLI help. Uploading returns a URL; application configuration and IAM acceptance
remain the separate existing save operation.

Uploads persist operation IDs and use leases to prevent concurrent retries.
Briefcase failure or lost responses can be retried with the original key and file;
the console retains that request for its Retry button. API tests cover current
admin gates, invalid/oversized images, metadata removal, idempotency conflicts,
revocation and storage recovery. Official SDK HTTP tests check exact body hashes,
organization selection, transfer capabilities and test authentication. Browser
tests verify create/edit persistence, image rendering and lost-response retry on
desktop and mobile. The CLI HTTP journey exercises upload and replay. Live
Briefcase provisioning and deployed acceptance remain outstanding.

Validation: workspace tests (39 backend API cases, two Briefcase SDK wire cases),
strict Clippy, production web build, CLI HTTP journey and all 32 desktop/mobile
browser cases passed. The logo screen was also visually inspected on mobile.
