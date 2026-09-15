# API contracts and compatibility

## Discover and select a contract

Read `GET /api/contracts` before choosing a version. This unversioned endpoint
stays available when a version retires. The existing `/api/v1/contract` alias is
retained for compatible v1 clients and follows v1's lifecycle.

```sh
curl -fsS https://backend.honeycomb.teamofsilicons.com/api/contracts
curl -fsS https://backend.honeycomb.teamofsilicons.com/api/v1/iam \
  -H 'Honeycomb-API-Version: v1' \
  -H 'Honeycomb-Client-Version: 0.1.0'
```

These URLs require production deployment. Use your local backend origin until then.
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
| Rust client 0.1.0 | v1 | 0.1.0 | Real client against an HTTP server: discovery, IAM metadata, typed catalog, authenticated status |
| Rust CLI 0.1.0 | v1 through Rust client | 0.1.0 | CLI HTTP journey: login, management, release, install/update/uninstall |
| Library/console 0.1.0 | v1 through website server | 0.1.0 | Desktop/mobile browser journeys |
| Existing raw HTTP callers | Explicit `/api/v1/` path | Header optional | Backend API regression cases without negotiation headers |

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
- Cargo patch/minor releases retain the current API contract. A client moving to a
  new API major version must declare it explicitly and update the matrix and tests.
- Raise a minimum-client policy only alongside tested migration instructions;
  do not use it as a replacement for maintaining a supported API contract.

Consumer tests run in `cargo test --workspace --locked`; the real-client integration
case lives in `crates/server/tests/api.rs`. CI also runs the complete CLI and browser
journeys. A producer-only JSON shape assertion is insufficient for compatibility.

## Deprecation and sunset

The `api_contracts` control table starts with **v1 active**. Active contracts never
retire automatically. Deprecation is an operator-controlled deployment change;
ordinary application users and testing keys have no contract-management endpoint.
Before deprecating an old version, implement and deploy its replacement, pass the
consumer matrix, publish migration instructions, and announce the change to users.
The current release has only v1, so it should remain active.

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
contract. Persisted timestamps survive process restarts. No fixed sunset date is
promised while traffic continues; every valid request moves the earliest date.

This essential lifecycle accounting stores one last-request timestamp per contract,
without identities, paths, payloads, tokens or request logs. It is independent of
optional usage telemetry. Tests cover active contracts, recent deprecation,
recent traffic, idle retirement, incompatible clients and post-retirement discovery.
