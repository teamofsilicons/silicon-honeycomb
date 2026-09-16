## Current contract
The current HTTP major is **v1**. The CLI and Rust client are **0.2.2**. Website clients use HTTP v1. Package manifests use **format_version: 1**. Each uploaded application independently uses semantic versions for its releases.

Call `GET /api/contracts` to discover supported versions. Responses identify `Honeycomb-API-Version` and `Honeycomb-Contract-State`. Existing raw callers may omit negotiation headers and use the version in their URL.

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

The current release implements only v1, so v1 remains active. Contract lifecycle accounting is essential server state and is independent of optional diagnostics.

In 0.2.0, `Manifest.app_id` is `Option<String>`. Manifests may omit `app_id`; an explicit value must match the selected application. Use `installer::install_archive_for` to supply the selected identity when installing an archive without it. Existing manifests remain valid, but older CLIs require the field.
