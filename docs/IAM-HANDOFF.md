# IAM changes required by Honeycomb

Status: proposed integration contract, not an assertion that these endpoints exist.
Based on the Honeycomb understanding and IAM client 1.9.0 inspected on 2026-09-15.
Route names below can be adjusted together before implementation; authority,
replay, isolation and activation semantics are required.

## What Honeycomb will own

Application catalog/details, CLI archives and releases, drafts, publication and
scope discussions, Honeycomb validator review, store/search/reviews/metrics,
and testing-environment lifecycle orchestration. IAM remains the authority for
identity, authentication, authorization, consent, accepted application security
configuration, scope decisions, secrets, OBO and test identity data.

## P0: application management integration (blocks real application activation)

Add a protected management API and expose it in the official Rust client.
Provision a dedicated integration identity for Honeycomb, separate from its
ordinary `tos>honeycomb` application secret.

Every user mutation must authenticate BOTH:

1. The narrowly scoped Honeycomb service identity (mTLS or dedicated service
   credential; normal application credentials cannot call these endpoints).
2. The acting Carbon/Silicon's Honeycomb-issued IAM user access token, with live
   membership/reviewer validation and a step-up assertion where required.

Use `Authorization: Bearer <integration credential>` and
`X-Honeycomb-Actor-Token: <IAM user access token>` for the proposed bearer scheme.
Never accept an unverified `actor_id` as authority. The actor must have current
`org_owner`/`org_admin` membership for configuration, or the exact provider/IAM
review authority for a scope decision. A service credential alone must not grant
user administration. Background lifecycle reconciliation uses an already accepted
operation, not a fabricated actor or a stored indefinitely-valid user token.

Proposed application endpoints:

| Method/path under `/api/v1/honeycomb` | Responsibility |
| --- | --- |
| `PUT /applications/{app_id}/configuration` | Create/link or change an application's desired security configuration |
| `GET /applications/{app_id}` | Accepted configuration, requested/effective scopes, approval and activation state, revisions |
| `POST /applications/{app_id}/secret-rotations` | Rotate secret with actor authority and step-up |
| `POST /applications/{app_id}/scope-decisions` | Accept/reject a reviewer decision for the exact app, scopes and request revision |
| `GET /operations/{operation_id}` | Reconcile a lost response and retrieve terminal or pending result |
| `GET /scope-catalog` | Current IAM permission descriptions, criticality, eligibility and reviewer authority |

Mutations carry `Idempotency-Key`, and a body such as:

```json
{
  "operation_id": "uuid",
  "configuration_revision": 1,
  "expected_iam_revision": 0,
  "app_id": "tos>example",
  "org_id": "tos",
  "name": "Example",
  "logo_url": null,
  "base_url": "https://backend.example.com",
  "visibility": "private",
  "availability": "active",
  "webhook": {
    "url": "https://backend.example.com/webhook/",
    "secret": "supplied signing secret, protected in transit and at rest",
    "scope": ["membership"]
  },
  "app_scope": {
    "iam": ["self.identity.read", "self.profile.read", "self.membership.read"],
    "external": []
  },
  "obo_endpoints": [],
  "obo_review_message": null,
  "testing_idle_days": 30
}
```

The response needs `operation_id`, `state` (`pending`, `accepted`, `rejected`),
`configuration_revision`, `iam_revision`, `effective_configuration`, structured
errors and outstanding required approvals. A public request is not effective
until IAM atomically verifies all required accepted scope decisions and the
separate authenticated Honeycomb publication approval. Keep the previous accepted
configuration active when a new proposal is rejected or pending.

Bind replay to integration identity + environment + actor + operation kind +
resource + exact body. Same key/different body must return conflict. Stale
expected revisions must return conflict; older operations must not overwrite
newer accepted configuration. Replays must not generate another secret or repeat
side effects. Return accepted operations after a lost response without requiring
the original actor token to stay alive indefinitely; re-evaluate authority before
any new security effect, and require renewed actor proof when it has expired.

### App secrets and bootstrap

IAM generates each new app_secret, stores only its verification hash and returns
the value only in the protected creation/rotation response. Support a documented
short encrypted replay window so transport retries recover the same secret.
Ordinary GETs and notifications must never reveal it. Rotations revoke previous
credentials according to the documented credential-version policy.

Bootstrap IAM and Honeycomb authentication records before Honeycomb's catalog
exists. A protected link/reconcile operation must reuse those same app_ids and
secrets, never silently create replacements or rotate them on retry. The initial
IAM login/signup and service startup cannot depend on a Honeycomb CLI release.

## P0: enforce accepted visibility and scope semantics

- Private apps: login, SLT exchange, refresh, introspection and OBO are restricted
  to current members and grants in the owning organization. Removal must take
  effect for already issued tokens. Private base URLs/catalog discovery must not
  leak to unauthorized callers.
- Private critical scopes bypass provider review ONLY. Declared scopes,
  eligibility, explicit user consent, endpoint availability, proof verification
  and actual resource permissions still apply.
- Going public: the private exemption is not an approval. Keep the app private
  until all critical IAM/OBO scope providers approve, Honeycomb validates and IAM
  accepts activation. Public apps adding scopes keep their old effective scopes
  while new ones await approval.
- Critical IAM decisions require IAM reviewer authority. Critical external
  decisions require the provider application's current owner/admin. Requester
  membership alone cannot approve a provider request. Cross-organization provider
  applications must work, subject to actor/resource permissions.
- OBO endpoint IDs have immutable paths; `ttl_seconds` defaults to 300 and must be
  positive. Proofs expire at the original issuance deadline or one successful
  verification. Retry cannot extend expiry. Endpoint disablement, scope removal,
  revocation and increased criticality take effect in exchange AND verification.
- Discover and request effective OBO endpoints directly through IAM at runtime.
  Honeycomb is not a runtime proxy.

## P0: live identity data needed by Honeycomb

Keep current SLT exchange, refresh, revoke, introspection and authorization
snapshots. Honeycomb will use the official Rust client, not collect IAM passwords
or verification codes. Website login goes through IAM-hosted consent and an
allowlisted callback with state binding. CLI login is `honeycomb login <slt>`.

Honeycomb needs disclosed principal identity, active selected organization
memberships and current owner/admin roles. Missing role disclosure must remain
missing, never default to admin. Also expose a live explicit capability such as
`honeycomb.applications.review` for platform validator decisions, or a dedicated
validated reviewer-authority endpoint. An ordinary owner of the platform org is
not automatically a Honeycomb validator.

Provide scope catalog and provider-review eligibility to Honeycomb application
callers/service integration without requiring a direct Carbon IAM token.
Provide scoped reviewer/owner/admin notification recipients (or IAM delivery by
recipient role) so Honeycomb can notify them without broad directory/email leaks.

Confirm the hosted IAM login URL, allowlist the library and console callback URLs,
and provision Honeycomb's app scopes/credentials. Keep browser app_secret-free.

## P1: signed notifications and reconciliation

Deliver to `https://backend.honeycomb.teamofsilicons.com/webhook/` with the official
client's raw-body signature scheme, timestamp/key version and replay protection.
Separate ordinary membership/profile subscriptions from management notifications.
Management events need `event_id`, resource ID, monotonically ordered resource
revision, accepted operation ID and event type. Include effective scope decisions,
revocations, visibility/availability, webhook activation and credential-version
changes. No secrets or user tokens in notifications.

Duplicate event IDs have no additional effect. Older revisions are ignored. A
revision gap triggers GET/reconciliation; notifications must not overwrite a newer
snapshot. Offer current-state reads and preferably a changes cursor so loss can
be recovered without guessing.

Test notifications preserve the signed outer `test` envelope from the understanding,
with environment_id and generation. Test root keys are redacted from durable event
payloads, logs and telemetry. Events from old cleaning generations are rejected.

## P1: testing lifecycle service API (blocks real shared test environments)

Honeycomb will generate environment IDs and cryptographically random 32-character
alphanumeric root keys, encrypt stored keys, maintain lifecycle operations and
per-service progress. IAM must accept these exact IDs and key versions through
protected control endpoints, independently of test sessions being cleaned.

Proposed `PUT /testing-environments/{environment_id}/operations/{operation_id}`:

- `prepare`: create/reuse the exact environment, organization identities and key
  version; import declared app snapshots; issue fresh test app secrets.
- `import` / `refresh-import`: preserve source org_id/app_id, configuration
  revision, chosen release, base URL, scopes, OBO TTL and webhook settings. Do not
  copy production users, sessions or app_secrets. Preserve a production webhook
  signing key internally if necessary, without disclosing it to the tester.
- `rotate-key`: install the new version and stop old-key authorization. Block
  affected test access during incomplete rotation.
- `clean`: block access, increment generation, erase test identity/session/proof/
  consent/import data, preserve environment identity and current key.
- `disable`: revoke test access and begin the 30-day restore window.
- `restore`: permitted before purge deadline, with readiness receipt.
- `purge`: permanently erase remaining environment data and key material.

Every operation has idempotency, expected environment revision, generation and a
service receipt; GET returns current state for lost-response recovery. Honeycomb
must await IAM AND each linked application before declaring lifecycle completion.
Expose validation of a production application's app_id/app_secret for app-owned
environment creation and management, with no escalation for merely attached apps.

All normal APIs accept `X-Testing-Environment-Key`; validate current active key
version/generation and same-environment identity, SLTs, tokens, app secrets and
OBO proofs. Invalid test context must never fall back to production. Test
credentials select a test flow but do not independently prove user identity.
Retain test-only `000000` verification with normal expiry/attempt/session checks,
and send no real email/SMS.

## P2: migration and ownership handover

Export/reconcile existing IAM-managed environments with IDs, creators, orgs,
current keys, app links, retention and active credentials preserved through a
protected transfer. Once reconciled, forward or compatibly retire IAM's previous
lifecycle management routes. Ordinary runtime test authentication remains in IAM.
Similarly transfer application configuration and scope decisions to Honeycomb as
the sole editing surface after reconciliation; avoid independent conflicting edits.

## Delivery and acceptance

Publish a new `silicon-iam-client` with typed management/lifecycle methods,
request/response models and structured error codes. Existing 1.9.0 runtime APIs
must remain backward compatible. Provide service credential setup and a local
test configuration so Honeycomb can run real integration tests.

Required cross-service tests: unauthorized service/actor rejection; stale member
rejection; Silicon admin support; private search/base URL/login protection;
private critical exemption; pending-to-public activation gate; provider review
and revocation; idempotency/body mismatch/stale revisions; lost secret response;
OBO TTL and one-use enforcement; cross-org OBO; invalid/mismatched test contexts;
key rotation/clean generation/restore/purge; lost receipt reconciliation; safe
bootstrap retry. Production activation is blocked until P0 is available; test
lifecycle completion is blocked until P1 and participating services are available.

### Import receipt details implemented in Honeycomb

The current proposed `import` lifecycle request sends `snapshot.imports` to IAM.
Each entry identifies app_id, org_id, source_revision, configuration_revision,
configuration, selected_release and initial private test visibility. Its completed
receipt includes an `imports` array with matching app_id/source_revision/
configuration_revision, a positive test iam_revision, the exact
effective_configuration and visibility=private. Honeycomb persists only the
nonsecret receipt identifiers. Production approvals and secrets are not copied.
An import adds per-app readiness gates while existing ready app instances remain
available. IAM must not activate newly imported app access until Honeycomb reports
all participating services ready. Retrying uses the same operation and snapshots.

### Secret operation details implemented in Honeycomb

Rotation requests carry operation_id, app_id, configuration_revision and
expected_iam_revision. Fresh step-up evidence is passed separately and is never
stored in the operation. IAM's accepted response must echo operation_id, app_id and
configuration_revision and return a newer iam_revision, positive credential_version
and one-time app_secret. The protected operation-result read echoes those identifiers
and can recover the same secret within IAM's replay window. After expiry, return
the accepted metadata without the secret; recovery must never perform a new rotation.
Honeycomb uses this result to reconcile a lost rotation response. Please provide
the hosted step-up initiation/callback contract for the console in the new SDK.
