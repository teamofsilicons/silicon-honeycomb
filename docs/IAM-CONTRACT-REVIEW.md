# Current IAM integration review

Reviewed local IAM commit `7abd575` (2026-09-16), SDK 1.10.0, against Honeycomb.
The current upstream source of truth is `silicon-iam/docs/HONEYCOMB_INTEGRATION.md`
and its OpenAPI contract. This review supersedes the earlier assumption that IAM
has no management API in `IAM-HANDOFF.md`.

## Available and wired locally

- Official `honeycomb::ManagementClient`, separate random `hck_` service credential,
  and live `oat_` actor header.
- Production application configuration: explicit desired configuration revision,
  expected IAM revision, stable operation ID/idempotency key, and private creation.
- App-secret rotation using `Mutation::step_up`; recovery replays the exact
  mutation within IAM's ten-minute secret window. The status GET is secret-free.
- Organization/provider-scoped permission discovery and the console picker.
- Exact pending-webhook approval and signing-secret rotation through the official
  SDK, Rust client/CLI and console, with fresh IAM step-up, actor-bound durable
  retries and encrypted signing material. Accepted rotation updates the locally
  retained secret used by later configuration changes.
- Accepted application reads: actual `app_name`, `app_logo`, `effective_scopes`,
  and `verified` availability are mapped to Honeycomb. Requested scopes are
  filtered through the effective scope list. Catalog-only description/docs fields
  stay Honeycomb-owned. The absent accepted webhook URL is not invented.
- A separate `/management/webhook/` receiver verifies the official SDK's
  `X-IAM-Management-Signature` over the raw bytes using the independent signing key.
  Application events queue durable revision reconciliation. Ordinary application
  webhooks retain their own endpoint, signing key and test-envelope verification.
- Exact wire bodies are encrypted before sending and retained for recovery.
  Step-up proofs and returned app secrets are never persisted.

Configuration: `IAM_HONEYCOMB_SERVICE_CREDENTIAL` enables the management adapter;
`IAM_HONEYCOMB_NOTIFICATION_SIGNING_KEY` enables the separate receiver. Without
these, management remains pending. Do not provision production cutover merely to
try the adapter: IAM provisioning retires its old management writers, while the
remaining Honeycomb flows below are not yet integrated.

SDK 1.10.0 was absent from crates.io when checked. The new commit is also ahead
of the remote default branch. `vendor/silicon-iam-client` contains unchanged SDK
source from the exact local upstream commit, an expanded standalone manifest,
license, and source hashes in `UPSTREAM.json`. Replace this with the published
registry dependency once available. No IAM source was edited or published here.

## Remaining contract gaps and work

| Area | Current IAM behavior | Needed next step |
| --- | --- | --- |
| App-owned environments | Protected prepare/import requires a user actor; no app credential verification/creation path in the service API | Verify production app credentials with IAM; create with creator_app ownership, attach by an existing key without granting ownership, and return only that app's test credentials |
| Shared root key | IAM prepare generates its root key; instruction has no supplied key/version | Agree who supplies the shared key and how every participant receives exactly that version; the understanding assigns generation to Honeycomb |
| Adding apps to a ready environment | Import only accepts prepared/cleaned IAM state; refresh requires disabling the whole environment | Add additive imports that preserve already-ready apps, or explicitly revise the product requirement |
| Test application administration | Production configuration rejects environment_id; generic management reads address production | Add isolated test configuration/secret/read operations and new test-only app registration |
| Scope review | Scope catalog and decisions exist; no immutable plan receipt or explicit current reviewer-capability query | Bind decisions to reviewed configuration/scopes, provide current IAM/Honeycomb validator eligibility, and map durable review plans without assuming membership grants validator authority |
| Public activation | Configuration uses publication_approved and current IAM revision; no separate request/plan activation receipt | Map Honeycomb's completed review to this write only after reviewer authority is wired; preserve exact request identity and verify current state before archive/catalog activation |
| Webhook accepted-state display | Approval and signing-key rotation are now wired through backend, CLI and console; the accepted record omits active destination URL | Return the authoritative active/pending URLs for display; current UI labels the local URL as requested |
| Recipient discovery | No scoped owner/admin/reviewer notification-recipient endpoint | Provide least-privilege recipients for queued Postmark notices |
| Lifecycle completion | IAM-local prepare/import/activate phases differ from Honeycomb's participant receipts | Add explicit IAM phase mapping, durable IAM revision/key/generation bookkeeping and cross-service activation; never turn IAM-only completion into shared readiness |
| Automatic retention | Honeycomb now schedules durable service-authorized cleanup, without a user token | Accept protected inactivity actions, exact retired-app IDs and revision/generation/key version; clear only those apps and their owned data across services, and return matching receipts |
| Legacy adoption | Inventory and legacy prepare/adoption exist | Preserve current IDs, keys, owners, links and accepted revisions; stage the writer cutover after migration verification |
| Test webhook SDK | Direct extra test.metadata fields still rejected | Support environment_id/generation there; current compatible placement is inside the signed aggregate |

The backend intentionally retains pending responses for these incomplete flows.
Service credentials are not substitutes for actor, app, root-key or reviewer
authority. Local HTTP-double tests validate the SDK wire mapping; they do not
establish live IAM deployment or cross-service readiness.
