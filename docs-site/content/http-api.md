## Discover the contract
```sh
curl -fsS https://backend.honeycomb.teamofsilicons.com/api/contracts
curl -fsS https://backend.honeycomb.teamofsilicons.com/api/v1/iam \
  -H 'Honeycomb-API-Version: v1' \
  -H 'Honeycomb-Client-Version: 0.1.0'
```
`/api/contracts` remains available when a version retires. The API path selects the major contract. An explicit version header must match it. See [Compatibility](/compatibility/) for lifecycle rules.

## Headers
| Header | Use |
| --- | --- |
| `Authorization: Bearer …` | Honeycomb user session for authenticated operations. |
| `Honeycomb-API-Version: v1` | Explicit contract selection matching the URL. |
| `Honeycomb-Client-Version: 0.1.0` | Semantic client version for minimum-version policy. |
| `Idempotency-Key` | Stable key for a logical mutation, 16–255 visible ASCII characters. |
| `If-Match` | Current integer resource revision for revision-guarded mutations. |
| `X-Testing-Environment-Key` | Explicit isolated environment root key when applicable. |
| `X-Honeycomb-Telemetry: false` | Opt out of optional diagnostics for this request. |

Do not send tokens to storage hosts or arbitrary redirect destinations. Use the official client when possible; it centralizes encoding, limits, and integrity checks.

## Read a catalog page
```sh
curl -fsS 'https://backend.honeycomb.teamofsilicons.com/api/v1/apps?q=briefcase&page=1'
```
Catalog pages contain `items`, `page`, `per_page`, and `total`. App records include `app_id`, `org_id`, `name`, `description`, `visibility`, `state`, configuration/IAM revisions, requested/effective config, latest release version, and aggregate community counts. Treat unknown response fields as compatible extensions.

Use percent-encoded path segments for application IDs: `my-org%3Emy-app`. An authenticated request can include accessible private results; anonymous results remain public.

## Creation and uploads
`POST /api/v1/apps` accepts the [AppInput JSON](/application-config/). Updates use `PUT /api/v1/apps/{id}` with a complete input and `If-Match`.

`POST /api/v1/apps/{id}/releases` accepts raw gzip archive bytes with `Content-Type: application/gzip`, not a multipart form. Include mutation headers. The maximum archive is 512 MiB. `POST /api/v1/packages/validate` validates the same package format.

Logo uploads use raw bytes with `Content-Type: application/octet-stream` at `POST /api/v1/organizations/{org}/logos`; limit 2 MiB. [Releases and assets](/releases/) covers image rules and visibility.

`GET /api/v1/apps/{id}/download?version=1.0.0` downloads an authorized release. Verify its byte length and SHA-256 against the release record, then validate the archive before extracting it.

## Errors and pending results
Errors use an `error` object with `code`, `message`, and optional `details`. Failed CLI commands exit nonzero; a raw HTTP client must check the response status. Authentication failures require a fresh valid grant, revision conflicts require reading current state, and integration failures require repairing the named dependency.

Cross-service mutations may return durable pending work. Inspect its operation ID rather than assuming the change is active. Exact bodies and types are defined in the [core models](https://github.com/teamofsilicons/silicon-honeycomb/blob/main/crates/core/src/model.rs) and [client methods](https://github.com/teamofsilicons/silicon-honeycomb/blob/main/crates/client/src/lib.rs).

## Registered routes
The following index is generated from the server router. It lists implemented routes, not a claim that every dependent production integration is complete. Authentication and role checks still apply to each handler.

<!-- ROUTES -->
