# Local checkpoint — 2026-09-16

This is a working local checkpoint, not a completed production rollout.

## Ready to review

- Rust backend, stateless client and stateful `honeycomb` CLI.
- curl/bash installer, Cargo source installation, binary updater and scheduled checks.
- Public/private library and authenticated console using the current Interface
  style: paper surfaces, blue accents and serif headings.
- Package validation, upload/install/update/uninstall, drafts, catalog/search,
  logo URL/file upload through Briefcase,
  reviews/stars, publication discussions/decisions/activation, secret recovery,
  permission picker, webhook approval/signing-secret rotation and testing-environment progress.
- Durable IAM configuration/reconciliation and protected SDK integration for the
  portions available in local IAM commit `7abd575`.
- Production container definitions, HTTPS routing for the requested domains,
  CI and six-platform release-candidate workflow.
- Space Station diagnostics, CLI/web opt-out, and isolated testing telemetry.
  The dedicated table is created and one synthetic ingestion was verified live;
  see [TELEMETRY.md](TELEMETRY.md).
- API contract discovery/negotiation, compatibility policy and persisted retirement
  after seven idle days for explicitly deprecated contracts; see [API-CONTRACTS.md](API-CONTRACTS.md).

Local previews currently run at http://localhost:4173 (library) and
http://localhost:4174 (console), using an explicit isolated IAM/storage fixture at
http://127.0.0.1:18080. Sign-in uses the fixture owner. These are sample identities
and sample applications; no production credentials or email delivery are involved.

## Verification

- Rust workspace tests and strict Clippy passed. Latest backend coverage: 44 API
  cases, plus runtime IAM, management SDK and Briefcase logo wire tests.
- All 36 desktop/mobile browser cases pass, including telemetry opt-out, logo upload/retry, webhook changes, retention
  settings and automatic cleanup progress/retry.
- CLI HTTP journey includes private creation, six-target archive validation,
  releases, install/execute/update/uninstall, review/activation and anonymous install.
- curl/bash installer checks and isolated Cargo source installation passed.
- Production backend and web images rebuilt successfully, including the current
  telemetry/lifecycle code and periodic reconciliation. Disposable container checks
  passed migrations, startup, catalog, authentication gate and both website modes.
  These checks use no live IAM credentials.
- All three distributable Cargo packages build with Cargo's temporary local registry.
- Both preview websites were visually inspected; source provenance hashes pass.

Cross-platform archive validation is not native execution on all six platforms.
The native six-platform release workflow has not run on remote CI.

## Remaining work before the complete product is ready

1. Resolve the implemented IAM contract differences in
   [IAM-CONTRACT-REVIEW.md](IAM-CONTRACT-REVIEW.md), then wire public review/activation,
   shared lifecycle phases, app-owned creation/attachment and test-only app management
   to actual protected services. The local fixture exercises these flows but does
   not prove live cross-service behavior.
2. Implement the services' automatic-retention transport and legacy adoption.
   Honeycomb now schedules durable app retirement, environment soft deletion and
   expiry purge; missing protected service acknowledgments keep these jobs pending.
3. Complete protected notification recipient discovery and live Postmark delivery.
4. Provision IAM/Briefcase permissions, dedicated service and notification keys,
   Postmark and deployment secrets; prepare the actual hosting/DNS and callback URLs.
5. Publish the repository, CLI binaries and crates, then run live browser/CLI/installer
   acceptance across the requested domains and supported platforms. No remote Git
   changes, crates, releases, domains or websites have been published here.

The actual IAM source now exists locally; the earlier blanket "IAM API absent"
assessment is superseded by the current contract review. SDK 1.10 is not yet in
crates.io, so the exact unchanged official source is pinned in `vendor` with hashes.
The production adapter keeps unsupported operations pending and exposes their errors.

See [IMPLEMENTATION.md](IMPLEMENTATION.md) for checkpoint history and
[../deploy/README.md](../deploy/README.md) for deployment/release preparation.
