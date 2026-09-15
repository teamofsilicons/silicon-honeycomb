# Current IAM integration review

Reviewed deployed IAM code commit `c57b3a3` and rollout documentation commit
`af194db` (2026-09-16), SDK 1.10.0, against Honeycomb.
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
- A persisted five-minute scan queues authoritative reads for known production
  applications with accepted IAM revisions, recovering missed notifications.
  Batches contain at most 50 apps; pending notification targets and retry schedules
  are preserved. This does not adopt unknown legacy applications or testing state.
- Exact wire bodies are encrypted before sending and retained for recovery.
  Step-up proofs and returned app secrets are never persisted.

Configuration: `IAM_HONEYCOMB_SERVICE_CREDENTIAL` enables the management adapter;
`IAM_HONEYCOMB_NOTIFICATION_SIGNING_KEY` enables the separate receiver. Without
these, management remains pending. Provisioning now leaves legacy management
writers available: the independent `IAM_HONEYCOMB_RETIRE_LEGACY_WRITERS` switch
defaults to false and remains false in production. Keep that switch off until
Honeycomb adoption and replacement flows are verified.

SDK 1.10.0 is still absent from crates.io (checked 2026-09-16). The deployed
release does not change SDK source or its manifest from `7abd575`.
`vendor/silicon-iam-client` contains unchanged SDK
source from the exact local upstream commit, an expanded standalone manifest,
license, and source hashes in `UPSTREAM.json`. Replace this with the published
registry dependency once available. No IAM source was edited or published here.

## Live rollout verification — 2026-09-16

- `/readyz` returns 200; `/api/v1/version` reports
  `c57b3a385fc6b0749f1cd9ae1e91e40169db531f`.
- An unauthenticated management catalog read returns 401. The provisioned
  service credential reads the catalog and `tos>honeycomb` with 200.
- The authentication app is verified/public, with IAM revision 2 and
  configuration revision 0. Its identity UUID is
  `01a0a751-69c0-70b2-8456-96806349f999`.
- The protected handoff is at
  `/Users/codanium/.config/silicon/honeycomb/iam-production.env` (0600), also
  held in AWS Secrets Manager as `silicon-honeycomb/production/iam-management`.
  Its environment variable names match Honeycomb's existing backend adapter.
  Credentials stay outside this repository.
- IAM notifications are queued pending Honeycomb's live HTTPS receiver.
  Scheduled testing remains disabled. The rollout does not resolve the contract
  gaps below or establish authenticated actor mutations through Honeycomb.

Repeat the read-only SDK check with
`python3 scripts/check_iam_management.py /Users/codanium/.config/silicon/honeycomb/iam-production.env`.
The helper parses dotenv values without shell evaluation; do not shell-source
this handoff because its unquoted `>` in the application ID is shell redirection.
It checks the real SDK's service authentication and response mapping without
printing credentials or mutating applications.

### Required bootstrap grants

The live app currently has only `self.identity.read` and `self.profile.read`.
Before authenticated application management, add effective `self.membership.read`
and require users to consent to it so role disclosure is available. The adapter
maps IAM's `owner`, `admin`, and `member` wire values to Honeycomb's existing
`org_owner`, `org_admin`, and `org_member` API values. Unknown or absent roles
grant no administrative authority.

Archive/logo operations also require the four external scopes on `tos>briefcase`:
`briefcase.uploads.reserve`, `briefcase.uploads.commit`, `briefcase.files.read`,
and `briefcase.link_access.update`. The last grants public link sharing and needs
the provider's approval. No external scopes are currently effective.
Add `--require-ready` to the check above to fail until all six required identity,
membership and storage scopes are effective. This is a prerequisite check, not
proof of user consent, resource authorization, or end-to-end storage success.

Also confirm unscoped token introspection discloses roles under
`self.membership.read`: the latest definition of
`iam_private.list_current_application_authorizations` found in migration 0072
still gates `org_role` on the retired `roles.read` scope, while the single-org
function updated by migration 0093 uses `self.membership.read`. Honeycomb's
website login does not pin an organization. This source discrepancy needs a
live actor regression check/fix in IAM; Honeycomb must not infer missing roles.

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
establish authenticated actor mutations or cross-service readiness. The live
read-only check above separately verifies the deployed management API.
