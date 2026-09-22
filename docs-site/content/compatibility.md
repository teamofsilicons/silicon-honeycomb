## Current contract
The CLI and Rust client are **0.3.3**. General APIs use **HTTP v1**; release-channel APIs use **HTTP v2**. Website release screens use v2 while other screens stay on v1. Package manifests remain **format_version: 1**. Each application maintains independent prod and dev release histories.

Call `GET /api/contracts` to discover supported versions. Its legacy `selected_version` remains `v1`; `release_version` identifies `v2`. Each version includes its scope, lifecycle state, and minimum client. `GET /api/v2/contract` selects v2 explicitly. Responses identify `Honeycomb-API-Version` and `Honeycomb-Contract-State`. Existing raw callers may omit negotiation headers and use the version in their URL.

## Negotiation failures
| Response | Meaning / recovery |
| --- | --- |
| `406 api_version_unsupported` | Header and route disagree, or the version is unsupported. Choose an advertised contract. |
| `400 client_upgrade_required` | Declared client is older than the minimum. Upgrade using migration guidance. |
| `400 invalid_request` | Malformed negotiation or request input; correct the request. |
| `410 api_version_retired` | The selected contract has retired; use supported discovery and migrate. |

There is no silent contract fallback.

## Compatible change policy
Within an API major, preserve existing required fields, types, routes, authorization meaning, error semantics, and idempotency behavior. Optional fields and new capabilities may be added. Callers should tolerate unknown response fields.

Removing a field/route, changing a field's meaning/type, or adding a required field needs a new major contract and migration guidance. A higher minimum-client policy is not a substitute for maintaining the advertised contract.

## Deprecation and retirement
Active contracts never retire automatically. Operators must first deploy a replacement, validate consumer compatibility, publish migration instructions, and announce deprecation.

A deprecated contract can retire only after **seven full days after both deprecation and its most recent qualifying request**. Valid version-matched traffic extends that interval, including authentication failures and declared clients needing upgrades. Health checks and unversioned discovery do not keep it alive. Retired contracts do not reactivate when another request arrives.

Both v1 and v2 initially remain active. V1 retains its minimum declared client of 0.1.0; v2 requires 0.3.0 when a client version is declared. Traffic, deprecation, and retirement are tracked separately: a v2 request cannot keep v1 alive or reactivate it. Contract lifecycle accounting is essential server state and is independent of optional diagnostics.

In 0.2.0, `Manifest.app_id` is `Option<String>`. Manifests may omit `app_id`; an explicit value must match the selected application. Use `installer::install_archive_for` to supply the selected identity when installing an archive without it. Existing manifests remain valid, but older CLIs require the field.


## Release-channel migration

V2 covers release listing/upload at `/api/v2/apps/{id}/releases`, download at
`/api/v2/apps/{id}/download`, and dev promotion at
`/api/v2/apps/{id}/releases/{version}/promote`. V2 uploads require an explicit
`channel=prod` or `channel=dev`; listing/download default to prod. V2 creation uses
numeric `x.y.z` versions. Promotion takes the new production version in its body.
Release responses include their channel. The version header must match the route.

V1 release listing and downloads retain the production-only view and legacy
response shape. Existing v1 uploads select production implicitly and retain their
semantic-version input compatibility. An upgrade does not reinterpret existing
production archives or require repacking them. The 0.3.0 Rust client's ordinary
release methods select v2; general authentication, catalog, and OBO calls stay on
v1. See [CLI reference](/cli-reference/) for channel selection and promotion.
