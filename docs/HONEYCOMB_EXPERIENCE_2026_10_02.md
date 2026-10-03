# Honeycomb experience update — 2 October 2026

This is local implementation and verification evidence, not a deployment record.
Human-owned `UNDERSTANDING.md` is unchanged by this work.

## Requests

- `GET /api/v1/sent-requests?page=1&per_page=50` returns requests for apps the current account manages, with pagination and partial-result status.
- `GET /api/v1/request-activity` returns `received_pending`, `received_unread`, `sent_unread`, and `partial`. It reads status without refreshing publication authority or triggering publication.
- Received inbox/details and sent history expose `activity_version`, `unread`, `created_at`, `updated_at`, and `message_count`.
- `POST /api/v1/requests/{id}/read` takes `view` (`sent` or `received`), `provider` for received requests, and the exact `activity_version` displayed. It requires authentication and an idempotency key. Markers are account-, environment-, request-, and provider-specific. Concurrent updates stay unread when an older snapshot is acknowledged.
- CLI equivalents: `honeycomb publication activity`, `sent`, and `mark-read`. `inbox --history` includes completed requests; `reply --request <id>` addresses the displayed historical request instead of silently selecting a newer one.
- UI polling retains usable cards during refresh errors. Counts distinguish a new update from an approval still needed. Email links preserve their destination through login.

## App pages and releases

The public app base path is `/apps/{app_id}`. It shows the current published production release; a newer private or development release must not silently become the public default. Exact releases have `/apps/{app_id}/releases/{prod|dev}/{version}` pages. Channel is part of the identity because both channels may use the same version.

Uploaded releases and published releases are distinct states. Upload receipts and emails link to the release page to inspect its review and publication status. Permission expansion continues to require review; existing public versions remain available while a new release is private.

## Logo upload and storage consent

The logo field previews an image and preserves the file and its idempotency key
while the user reviews separate Briefcase storage permission. Completing the
bound IAM callback resumes the same upload. Closing or rejecting the permission
window leaves an actionable retry; invalid or revoked permission asks for consent
without signing the user out of Honeycomb.

The backend exposes `POST /api/v1/storage-authorizations`,
`GET /api/v1/storage-authorizations/{id}` and
`POST /api/v1/storage-authorizations/{id}/complete`. Browser return URLs are bound
to Honeycomb's configured origin and fixed callback path. CLI callers omit the
return URL and complete with the one-use code and matching state. Every request
is isolated by requesting account, organization, data plane and environment
generation. Stored OBO access/refresh credentials are encrypted, and downstream
operations use the organization selected for Briefcase during consent.

CLI equivalents are `honeycomb apps storage start`, `status --wait` and
`complete --code-file`. An `apps upload-logo` permission failure starts the
manual consent flow and returns the original upload retry key.

## Email

Local changes add HTML and plain-text templates, escaped untrusted content, direct request/version links, disabled tracking, visible missing-provider queue errors, and positive Postmark receipt reconciliation for uncertain sends. A missing search result never authorizes a duplicate send. Review emails for completed or superseded requests, and outdated status emails, are suppressed before sending. Testing environments capture notifications locally.

Production startup validation now requires `POSTMARK_SERVER_TOKEN` in the protected backend runtime map before stopping existing containers. Do not commit credentials. Review existing pending and held events before enabling mail: enabling the provider lets the worker process pending events immediately.

### Read-only live diagnosis

On 2 October 2026, AWS SSM inspection of the running Honeycomb backend confirmed:

- No Postmark or mail-related environment variable was present. `POSTMARK_SERVER_TOKEN` was absent.
- 96 production events were pending, all with zero send attempts: 13 review requests, 32 status updates, 51 uploaded releases.
- 159 older events were in `held_identifier_migration`: 2 discussions, 28 review requests, 67 status changes, and 62 releases.
- No outbox error explained the missing mail configuration. The existing worker silently returned when its provider was absent.

Evidence: read-only SSM commands `62b6d475-4040-48a7-a660-fa4f2b0c1d45` (queue aggregates) and `70d2ccd0-6ea0-47ec-8f37-06ed5d8b3cb1` (credential presence only). No credentials, message contents, or recipients were printed. No events were released and no mail was sent.

Before production rollout, supply a valid Postmark server token through the protected runtime configuration and confirm its approved sender. Select a backlog policy explicitly rather than automatically replaying stale requests or migration holds. Then verify a newly authorized notification using its provider receipt and destination link.

## Validation so far

- Rust workspace: 228 tests passing with no ignored tests; Clippy passes with warnings denied. Identifier migration tests: 13 passing, including an upgrade with existing immutable holds.
- Server API integration suite: 80 passing tests, including request read-state isolation, stale-acknowledgement safety, completed history, historical replies, publication/approval workflows, logo upload validation and replay, notification privacy boundaries, stale-email suppression, and missing-provider recovery.
- Notification adapter/template unit tests: 3 passing, with mock HTTP receipt reconciliation and HTML/link escaping.
- Storage integration: 7 passing protocol tests plus 2 client wire tests, including callback validation, encrypted credentials, selected provider organization, refresh retry and production/testing separation.
- Honeycomb frontend build and 18 tests passed. Desktop/mobile browser evidence is in `.codex-artifacts/honeycomb-audit-2026-10-02` at the shared Silicon workspace root. The logo fixture resumed the same file and idempotency key across two upload attempts.
- All 60 desktop/mobile browser cases passed across the final full run and targeted reruns after obsolete selectors and test-fixture values were corrected. Release coverage includes exact version URL reloads, production install defaults, development opt-in, and private-until-approved updates.
- IAM integration checks are recorded separately; a passing local mock is not proof of live email delivery or provider login.
