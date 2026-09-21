# Default public approval requests

New production application configurations default to `config.visibility: public` (publication intent). The application's top-level visibility and IAM's effective visibility remain private until all required provider/IAM/validator approvals and archive activation finish. `visibility: private` explicitly opts out; omission during an update preserves the stored preference, or a legacy application's current visibility. Changing this preference does not unpublish an already public application.

A valid CLI release and accepted configuration trigger a single review request per application revision. Configure/upload replays reuse it. The application description supplies the initial review message. Publication errors do not hide an accepted upload or a one-time application secret. Pending configuration or review-plan outages retry through a bounded background queue using an encrypted, revocable manager access token. Tokens expire from that queue after 24 hours; a manager can retry the original operation to renew authorization. Current authority, principal identity, application revision and production plane are checked before submission. Test applications never auto-submit production requests.

The console defaults to public and offers an explicit private preference, refreshes the review after upload and hides duplicate submission forms. The deployed HTTP API accepts the preference. CLI source builds accept the JSON field; public CLI 0.2.2 predates it, so the console is the supported private opt-out for that binary until a new CLI release is distributed. Existing private applications were not bulk-published.

## Deployment

- AWS account `234951665042`, profile `silicon-production`, region `us-east-2`, instance `i-06986627793021fb2`.
- Backend image `234951665042.dkr.ecr.us-east-2.amazonaws.com/silicon-honeycomb-backend@sha256:47b644c79779ba678bae14ed9bbb0c0a215cd8e040d2070a457724e28221ca75`; tag `default-public-20260917`.
- SSM rollout `9e6e0c1a-7443-4820-84e5-ba3c1572309a` succeeded, guarded against replacing a concurrently changed image.
- SQLite backup verified with integrity_check: `/var/lib/silicon-honeycomb/backups/default-public-20260916/honeycomb-1789588014.db` (UTC-date folder retained from rollout template).
- Rollback container retained: `honeycomb-backend-before-default-public-1789588014`.
- Migration 22 adds the publication-intent retry queue; existing rows keep their visibility.
- Console: `https://silicon-honeycomb-console-1s34vyx74-saketdev12-5675s-projects.vercel.app`, aliased to `https://console.honeycomb.teamofsilicons.com`.
- Library: `https://silicon-honeycomb-library-jtm3wnust-saketdev12-5675s-projects.vercel.app`, aliased to `https://honeycomb.teamofsilicons.com`.
- Documentation deployed to `https://docs.honeycomb.teamofsilicons.com`.

## Verification

- `cargo test --workspace --locked`: passed, including 69 API integration tests and six focused default-publication cases covering validated upload/replay, private opt-out/legacy preservation, IAM retry, reconciliation resumption, revoked/superseded/expired authority and invalid preferences.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo fmt --all --check`, `git diff --check`: passed.
- Web TypeScript/Vite build and desktop registration/upload-retry + draft-save journeys: passed. The registration journey verifies one request and still-private effective visibility.
- Documentation build/check: 26 pages, 1489 links/assets. Generated reference: 70 CLI help pages, 59 API methods. Downloadable package examples pass structural validation.
- Public backend health returns ok. Live API rejects an invalid visibility with the expected field-level error before creating any application.
- Both production UI assets contain the new preference. Authenticated live console creation form shows public selected by default and private available; verification did not create an application or save a draft.
- Starter publication GET returns an empty request list. Per the user's correction, Starter remains private and is excluded from the Interface bundle; see the interface-bundle release follow-up for its accepted seven-member configuration.

Changes are left in the existing working tree alongside previously pending work; no unrelated edits were reverted or committed.
