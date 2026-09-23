# Public identifier release candidate — 2026-09-23

Honeycomb 0.4.0 is a staged candidate for bare application IDs, typed Carbon and Silicon IDs, and explicit organization ownership. It is not deployed or published by this preparation step. Follow [the migration rules](../../docs/IDENTIFIER-MIGRATION.md) before reopening writes.

## Source and live parity

The candidate integrates the live backend source `8660b2bc0d73b5576e9167d4cc9c8e0eef9608d5`, including application-owned environment replay compatibility, imported testing-configuration revision translation, credential-rotation recovery, and public legacy-adoption reconciliation. It also retains the deployed CLI update/installer fixes. The source branch is `release/public-identifiers-20260923`; the commit containing this record and its follow-ups identify the exact candidate.

The live ARM64 backend at preflight was `silicon-honeycomb-backend@sha256:74f61449a04a38b9f93bcf88c9c8c81fadf48fbaf9835d80ee9a6ffb14567fa6`, tagged `0.3.3-8660b2b`. Live CLI discovery returned 0.3.3. The local SQLite migration checksums match all 25 applied live migrations. Their original numbers and bytes are retained; the new schema is migration 0026.

AWS account `234951665042`, region `us-east-2`, stack `silicon-honeycomb-production`, and SSM host `i-06986627793021fb2` were reachable. The host reported approximately 28 GB free. Local Docker ARM64, GitHub authentication and AWS SSO were available. No infrastructure changes were performed.

## Validation

The combined source passed the locked Rust workspace suite, formatting, vendored IAM source verification, web production build and all 12 web unit tests. Offline SQLite migration tests cover collision/ownership failures, active-work fences, installed-registry preservation and operation-bound app-create hash contexts. App-owned environment regressions verify retained world IDs, key bytes and valid retries after both UUID and qualified public-ID migration.

Local evidence is retained outside the repository in `/tmp/honeycomb-live-integration-final.log`, `/tmp/honeycomb-namespace-owner-retry.log`, `/tmp/honeycomb-live-web-build.log`, and `/tmp/honeycomb-live-web-unit.log`. GitHub CI on the candidate must pass before release.

## Cutover gates

The database preflight found pending operations, an unresolved publication request and pending notification records. These remain intact. Resolve ordinary operations through their supported retry/reconciliation/lifecycle paths; publication decisions retain normal actor authorization. Notification dispatch requires the configured Postmark integration. Do not mark records complete or delete queues to satisfy the migration fence.

Build backend and gateway images for `linux/arm64` from the committed candidate, label each with its source revision, and stage immutable ECR tags/digests. Build the six CLI targets and use the matching Honeycomb packager for bare application manifests. `deploy/aws/deploy.py` is a mutating deployment helper that also reconciles shared Briefcase credentials; it is not a preflight command.

During the coordinated cutover, stop writers, back up each SQLite database with SQLite's backup API, retain the matching encryption/session keys, and verify the backup. Apply schema 0026 followed by the explicit IAM mapping in preview and apply modes. Update typed runtime application settings and preserve all unrelated secrets. Only then start the candidate and verify existing package downloads, old-archive installation, app-owned environments and authorization. No database migration, image publication, frontend publication or service restart was performed during this preflight.
