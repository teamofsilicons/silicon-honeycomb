# API contracts and compatibility

## Discover and select a contract

Read `GET /api/contracts` before choosing a version. This unversioned endpoint
stays available when a version retires. The existing `/api/v1/contract` alias is
retained for compatible v1 clients and follows v1's lifecycle. Its
`selected_version` remains `v1`. Discovery advertises both supported majors,
per-version `scope`, and `release_version: "v2"`. The
`/api/v2/contract` endpoint selects the release contract explicitly.

```sh
curl -fsS https://backend.honeycomb.teamofsilicons.com/api/contracts
curl -fsS https://backend.honeycomb.teamofsilicons.com/api/v1/iam \
  -H 'Honeycomb-API-Version: v1' \
  -H 'Honeycomb-Client-Version: 0.3.0'
```

Use your own origin for local validation. A deployed backend's discovery response
determines which contracts are available there.
The response lists implemented, non-retired `supported_versions`, per-version
state/minimum-client policy, and the seven-day retirement interval. The Rust client
and website server declare their API contract and package version automatically.
Responses include `Honeycomb-API-Version` and `Honeycomb-Contract-State`.

The path selects the major API contract. An optional version header must agree:
`v2` sent to a v1 endpoint returns HTTP 406 with `api_version_unsupported`, before
the endpoint executes. An invalid or too-old declared client version returns HTTP
400 (`invalid_request` or `client_upgrade_required`). Retired contracts return
HTTP 410 (`api_version_retired`). There is no silent fallback to another contract.
Existing clients that omit these headers continue using the version in their URL.

## Compatibility matrix

| Consumer | API | Minimum declared client | Current verification |
| --- | --- | --- | --- |
| Rust client 0.3.0 | v1 general; v2 releases | v1: 0.1.0; v2: 0.3.0 | Real client/server tests: discovery, IAM metadata, typed catalog, authenticated status, separate release channels and promotion |
| Rust CLI 0.3.0 | v1 and v2 through Rust client | v1: 0.1.0; v2: 0.3.0 | CLI HTTP journeys: login, management, release, exact-version install, channel switch, update and uninstall |
| Library/console 0.3.0 | v1 general; v2 release screens | v1: 0.1.0; v2: 0.3.0 | Browser journeys for channel selection, release history and promotion |
| Legacy raw HTTP callers | Existing `/api/v1/` paths and production release view | Header optional; 0.1.0 when declared | Raw v1 release tests preserve response shape, semantic versions and saved upload idempotency keys |
| Delegated application callers | v1 `honeycomb.apps.list` | Header optional; 0.1.0 when declared | Real client/server/IAM-SDK tests verify proof binding, private inventory access and isolated test context |

This matrix describes local compatibility tests. Live IAM, release publication
and production acceptance remain separate gates. Future consumer/version rows
must have evidence before they are advertised as supported.

## Version policy

API major versions, Cargo semantic versions, application configuration revisions,
IAM revisions and uploaded application release versions are separate counters.

- Preserve existing routes, required fields, field types, authorization behavior,
  error-code meaning and mutation/idempotency semantics within an API major version.
- Compatible changes may add optional request fields, response fields, routes or
  capabilities. Consumers must tolerate unknown response fields.
- Removing/renaming fields or routes, changing existing types/meaning, or requiring
  additional request fields needs a new API major version and migration guidance.
- Cargo versions and HTTP majors are independent. A client selecting a new HTTP
  major must declare it explicitly and update the matrix and tests. Existing
  advertised HTTP contracts keep their behavior during that migration.
- Raise a minimum-client policy only alongside tested migration instructions;
  do not use it as a replacement for maintaining a supported API contract.

Consumer tests run in `cargo test --workspace --locked`; the real-client integration
cases live in `crates/server/tests/api.rs`, `release_channels.rs`, `contracts.rs`,
and `obo.rs`. CI also runs the complete CLI and browser journeys. A producer-only JSON shape assertion is insufficient for compatibility.

## Deprecation and sunset

The `api_contracts` control table starts with **v1 active** and **v2 active**.
V1 retains its 0.1.0 minimum declared client; v2 uses 0.3.0. The v2 migration
preserves any operator-configured v1 policy, timestamps, and state. Active
contracts never retire automatically. Deprecation is an operator-controlled deployment change;
ordinary application users and testing keys have no contract-management endpoint.
Before deprecating an old version, implement and deploy its replacement, pass the
consumer matrix, publish migration instructions, and announce the change to users.
Both versions remain active until explicitly deprecated; adding v2 does not
deprecate or retire v1.

At that future deployment, a reviewed database migration marks the older contract
`deprecated` and sets `deprecated_at` to the current Unix timestamp. Do not backdate
it or delete its usage history. The server then reports `deprecated` on responses.
Retirement requires seven full days after **both** deprecation and the most recent
well-formed, version-matched request. Auth failures and declared clients requiring
an upgrade still count as traffic. Discovery at `/api/contracts` and health checks
do not keep an old contract alive. Requests with unsupported version negotiation
or malformed client-version headers do not count as users of that contract.

The server checks retirement every minute, during discovery, and atomically with
request admission. A request admitted before the boundary resets the interval.
After retirement, the first attempted request gets 410 and cannot reactivate the
contract. Each major has its own timestamps and retirement checks: requests to
v2 cannot keep v1 alive, and vice versa. Persisted timestamps survive process restarts. No fixed sunset date is
promised while traffic continues; every valid request moves the earliest date.

This essential lifecycle accounting stores one last-request timestamp per contract,
without identities, paths, payloads, tokens or request logs. It is independent of
optional usage telemetry. Tests cover active contracts, recent deprecation,
recent traffic, idle retirement, incompatible clients and post-retirement discovery.

## Release contract migration

V2 owns `/api/v2/apps/{id}/releases` (GET/POST),
`/api/v2/apps/{id}/download` (GET), and
`/api/v2/apps/{id}/releases/{version}/promote` (POST). V2 upload requires an explicit
`channel=prod` or `channel=dev`; listing and download default to prod. V2 release
creation uses numeric `x.y.z` versions. Promotion creates a production archive
with the requested production version while preserving executable bytes.

V1 release routes remain production-only and retain their original response
shape, implicit production upload behavior, semantic-version acceptance, and
idempotent retries. Existing archives keep their bytes and checksums. The 0.3.0
Rust client's release methods and the website's release screens select v2;
general catalog, authentication, package validation and OBO routes remain on v1.
Dev/prod channels are independent of `X-Testing-Environment-Key`, which always
selects the data/authentication plane. V2 is not an alias for all v1 endpoints.

## Organization application inventory

`GET /api/v1/organizations/{org}/apps` requires a current Honeycomb user membership
in that organization. `POST /api/v1/obo/apps/list` exposes the same inventory to
other applications under the critical `honeycomb.apps.list` OBO permission.
The POST body is `{org_id, after?, limit?}`; the GET uses `after` and `limit` query
parameters. Limits are 1–100, default 100. Both return `{org_id, items, next_cursor}`
with stable app-ID ordering. Each item contains exactly `app_id`, `org_id`, `name`,
`description`, `visibility`, and `state`. Private and inactive catalog entries
remain visible to that organization's members; app execution/download eligibility
is checked separately. Configuration and secrets are excluded.

The OBO handler accepts `X-IAM-OBO-Access-Token` containing the dedicated OBO
access token from a separately approved grant. Honeycomb authenticates its own
recipient application to IAM's `POST /api/v1/obo-access/token-verifications`
with `{access_token, endpoint_id, request:{method,path}}`. It checks the active
recipient, endpoint, selected organization, delegated scope, actor consistency,
and environment, then applies the inventory's resource checks. Verification is
repeatable: the same unexpired token can serve multiple pages and retries, with
current authority checked on every request. The token is not bound to a body
digest and is not consumed. Ordinary application-login tokens cannot replace it.

The consent review covers the full declared dependency graph, with an approved
account and organization for each provider. A provider can forward the same token
to a declared downstream endpoint, whose receiving app verifies it using its own
credentials. Consent remains until revoked; access-token expiry, security epochs,
current membership, current graph and testing generation still apply.
`X-Testing-Environment-Key` selects an isolated data/authentication plane;
invalid/mismatched context fails closed. `/api/contracts` advertises the provider
definition as `obo_endpoints`. Deployment must synchronize
`deploy/honeycomb-obo.json` through the normal application configuration workflow
before callers can request and approve the endpoint.

## Application-to-application verification

The October 2026 IAM redesign adds `ata_endpoints` to application configuration.
Each definition has a local `endpoint_id`, `name`, `path`, `critical`, `description`,
object `metadata`, optional `note_to_user`, `additional_warnings`, `downstream`
ATA dependencies, and `enabled`. IAM discovery returns its globally unique
`[app_id:ata:local_id]`. Importing an OBO definition copies its presentation and
path; review and add ATA dependencies explicitly. OBO credentials and user
contexts are never copied into ATA.

Honeycomb exposes these authenticated manager routes:

| Method | Path | Result |
| --- | --- | --- |
| GET | `/api/v1/ata-verifications` | Current signer's records across manageable apps, app choices, and explicit per-app load failures; no token plaintext |
| GET | `/api/v1/apps/{app}/ata-verifications` | Metadata, current status, immutable signer; no token plaintext |
| POST | `/api/v1/apps/{app}/ata-verifications/preview` | Complete application/endpoint graph and its `graph_version` |
| POST | `/api/v1/apps/{app}/ata-verifications` | Verification metadata and one-time refresh token |
| POST | `/api/v1/apps/{app}/ata-verifications/{id}/revoke` | Revocation invalidates this verification's refresh/access credentials |

Create/revoke require a UUID `Idempotency-Key`. Reuse that same key and exact body
when retrying an uncertain response. Honeycomb supplies the IAM operation identity;
callers must not put `operation_id` in the public Honeycomb body. IAM is the token
authority, while Honeycomb forwards the current manager's IAM proof. Credentials
are not saved in configuration archives.

Preview accepts `endpoints: [{audience, endpoint_id}]`, optional `app_ids`,
`expires_after` (seconds, at least 3600; omitted/null means never), and
`access_token_validity` (60–86400 seconds, default1800). Review every dependency,
then create with the returned complete `app_ids`, complete `endpoints`, and
`graph_version`, plus the chosen lifetimes. A changed graph requires a fresh review.
Management currently uses the production application console; a testing-key
context cannot mint a production credential.

The CLI has the same flow:

```sh
honeycomb apps ata list
honeycomb apps ata list ting
honeycomb apps ata preview ting request.json
honeycomb --idempotency-key <uuid> apps ata create ting reviewed-request.json
honeycomb --idempotency-key <another-uuid> apps ata revoke ting <verification-id>
```

The console's **App to App verifications** page uses the personal list. Its
`items` contain only records signed by the current principal; `applications`
lists current owner/admin app choices, and `failures` identifies apps whose
records could not be loaded. Partial results must not be presented as complete.
Omitting the app ID in the CLI lists this same workspace. Supplying an app ID
retains the manager view of all its verifications. Creation still selects an
originating application and uses its existing manager authority; endpoint
definitions remain in each application's configuration.

Save the returned refresh credential in the application's secret store. The
originating app exchanges it at IAM `POST /api/v1/ata-access/tokens` using its own
application credentials. Refresh rotates the credential; reuse invalidates the
verification. A receiving app uses its own credentials at
`POST /api/v1/ata-access/verify` with the originating `app_id`, `app_proof_token`,
and exact `endpoint`. One access token covers the reviewed ATA chain, and it
never conveys Carbon, Silicon, membership, or OBO authority.

These are local implementation contracts. Production activation and coordinated
consumer migration remain release gates.


## Request activity and read state

See [the experience contract](HONEYCOMB_EXPERIENCE_2026_10_02.md) for sent-request pagination, read-only activity counts and account-isolated exact-version read markers. New routes use the existing v1 authentication, testing-plane and mutation-header contracts.
