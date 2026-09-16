# Automatic testing credential deployment — 2026-09-16

Honeycomb now provisions and verifies its Briefcase lifecycle service credential
as part of managed deployment. App owners do not generate or copy a token. The
backend uses the credential for authenticated, retry-safe lifecycle requests and
checks exact operation/environment/revision/generation/key-version receipts.

## Source and backend deployment

- Automatic provisioning: `4b32f80`.
- Briefcase lifecycle transport: `9824b0e`.
- Image: `234951665042.dkr.ecr.us-east-2.amazonaws.com/silicon-honeycomb-backend@sha256:2234baa2e5e367edfed3947b8b7a0f3bb2aa87b8544c56df48a7565b034e80ff`.
- All 108 recorded runtime build inputs matched the committed source.
- AWS profile `silicon-production`, region `us-east-2`, host `i-06986627793021fb2`.
- Successful SSM rollout: `7c6e602f-6f1b-4092-948e-21d574a728c2`.
- Consistent SQLite backup and previous backend configuration:
  `/var/lib/silicon-honeycomb/backups/lifecycle-20260916T034109Z`.
- Only the backend container was replaced. Library, console and proxy container
  IDs were preserved; existing database and encryption keys were preserved.
- Health-gated rollout had an automatic previous-image/configuration rollback.
  Rollback was not needed.

## Checks

- Server tests: 64 passed; formatting and strict server Clippy passed.
- Automatic credential provisioning: 5 tests passed; production verification
  found matching stored credentials and made no changes or rotations.
- Official vendored IAM source verification passed.
- Dependency advisory, ban and source checks passed. The optional default
  `cargo deny check` license pass failed because this repository has no deny
  license allowlist; it rejected even MIT dependencies. No dependency change was
  introduced in this release, and license approval is not claimed by that run.
- Public backend health, Briefcase catalog entry, and both website configuration
  endpoints returned HTTP 200. Briefcase remained public and active.
- Protected Briefcase HTTPS receipt lookup: anonymous request returned 401;
  configured service credential returned 404 for a random nonexistent operation,
  confirming authentication without creating or mutating an environment.
- Existing isolated CLI session returned 401/login required. Authenticated CLI
  continuity is not counted as passed; public CLI archive validation passed.

## Docs update and boundaries

`2bd974c` removes the Availability & current limits page, its navigation entry,
search/sitemap/Markdown output and all internal links. The remaining 25 pages and
links validate; all 12 desktop/mobile documentation browser tests passed.

This deployment enables Briefcase service transport. It does not complete IAM's
shared-testing lifecycle contract: shared keys, activation phases, revisions,
active imports and exact retirement still require coordinated IAM changes. The
adapter continues to report unavailable rather than falsely marking an environment
ready. The planned Briefcase archive is still unuploaded.

Docs production deployment:
`https://silicon-honeycomb-docs-o0eksurck-saketdev12-5675s-projects.vercel.app`, aliased to
`https://docs.honeycomb.teamofsilicons.com`. Live home/search/sitemap returned 200
with no removed-page links; search contains 25 pages. Both `/availability/` and
`/markdown/availability.md` return 404.
