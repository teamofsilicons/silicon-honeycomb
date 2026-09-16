# Current IAM integration review

Reviewed deployed IAM code commit `ed9abe2` (2026-09-16), SDK 1.10.0,
against Honeycomb. The original management rollout was `c57b3a3` / `af194db`.
The current upstream source of truth is `silicon-iam/docs/HONEYCOMB_INTEGRATION.md`
and its OpenAPI contract. This review supersedes the earlier assumption that IAM
has no management API in `IAM-HANDOFF.md`.

## Action list for the IAM rollout

### Now: authenticated application creation

1. `self.identity.read`, `self.profile.read` and `self.membership.read` are now
   effective on `tos>honeycomb` (IAM revision 3). Renew user consent when signing
   in. Existing sessions must not gain undisclosed authority automatically.
2. Ensure both organization-scoped and unscoped introspection disclose current
   `owner` / `admin` / `member` roles with that membership grant and consent.
   Migration 0099 fixes `iam_private.list_current_application_authorizations`
   to require consented membership access (details below). A fresh production
   Carbon sign-in now enables Create application for `tos`; an authorized draft
   save and read after a full page reload both passed. Missing disclosure still
   correctly keeps Honeycomb's Create application button disabled.
3. Exercise a real owner/admin actor through private application creation with
   the existing Honeycomb management service credential. Confirm that members,
   revoked memberships and service-only requests cannot perform that mutation.
   The management service credential is already provisioned; no replacement is
   needed merely to add the membership scope.
4. Configure queued management notifications to the now-live receiver at
   `https://backend.honeycomb.teamofsilicons.com/management/webhook/`, using the
   existing independent management notification signing key. Verify signed
   delivery, retry and authoritative revision reconciliation.

### After Briefcase setup

Add `self.tags.read` to Honeycomb's declared/effective IAM scopes and renew user
consent. Briefcase rejects a delegated authorization snapshot with undisclosed
tags, and IAM migration 0093 only discloses tags when the issuer token, issuer
approval, audience approval, and user consent permit it. The earlier four-scope
storage checklist omitted this additional disclosure requirement.

Add external scopes on provider `tos>briefcase`: `briefcase.uploads.reserve`,
`briefcase.uploads.commit`, `briefcase.files.read`, and
`briefcase.link_access.update`. Obtain the required provider approval and user
consent. These support logo/archive storage and public links; they are deferred
and are not the cause of the disabled application-creation button.

### Remaining IAM contract implementation

The table below gives the exact gaps: app-owned test environments and credentials;
Honeycomb-generated shared keys; additive imports into ready environments;
isolated test-app administration; immutable review plans and current reviewer
authority; publication activation receipts; authoritative webhook destinations;
scoped notification recipients; lifecycle receipts and service-authorized
retention; legacy adoption; and test webhook metadata. Keep legacy writers and
scheduled testing cutovers disabled until their replacement flows pass live E2E.

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

- `/api/v1/version` now reports
  `ed9abe2241fb8962c6036624d1946ab7c7f79068`.
- An unauthenticated management catalog read returns 401. The provisioned
  service credential reads the catalog and `tos>honeycomb` with 200.
- The authentication app is verified/public, with IAM revision 3 and
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

The live app now has `self.identity.read`, `self.profile.read` and
`self.membership.read`. Users must renew consent to receive membership disclosure;
the live IAM consent screen now displays all three permissions. The adapter
maps IAM's `owner`, `admin`, and `member` wire values to Honeycomb's existing
`org_owner`, `org_admin`, and `org_member` API values. Unknown or absent roles
grant no administrative authority.

Archive/logo operations also require `self.tags.read` and the four external scopes on `tos>briefcase`:
`briefcase.uploads.reserve`, `briefcase.uploads.commit`, `briefcase.files.read`,
and `briefcase.link_access.update`. The last grants public link sharing and needs
the provider's approval. No external scopes are currently effective.
Add `--require-ready` to the check above to fail until all seven required identity,
membership, tags and storage scopes are effective. This is a prerequisite check, not
proof of user consent, resource authorization, or end-to-end storage success.

IAM commit `ed9abe2` adds migration 0099 to fix unscoped role disclosure. It
requires `self.membership.read` in the issued token, current app approval and
live user consent; retired `roles.read` cannot disclose roles. Its SQL regression
tests cover old-token isolation, renewed consent and approval revocation. The
new-app bootstrap also includes membership access, while existing identities
must use the authorized configuration flow. Honeycomb's website login does not
pin an organization. Renewed production consent for `tos` now passes the owner/
admin UI gate and backend draft write/read authorization. The saved verification
draft is `honeycomb-e2e-20260916` (Honeycomb E2E verification); it has not been
submitted as an IAM application. Honeycomb never infers missing roles or reuses
earlier consent as a new grant.

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
