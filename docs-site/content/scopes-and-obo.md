## Requested versus effective
`app_scope` describes what your app requests. IAM decides what is accepted and currently effective. A user must also consent to the requested account/organization data. Honeycomb saves requested configuration and displays accepted state separately.

## Request access to another application
```json
{
  "app_scope": {
    "iam": ["self.identity.read", "self.membership.read", "self.tags.read"],
    "external": [
      {"app_id": "briefcase", "endpoint_id": "briefcase.files.read"}
    ]
  }
}
```
This is a configuration fragment, not a full creation document. Choose only what your app needs. External endpoint IDs come from the provider's registered catalog. For Briefcase, the delegated effective scope is represented as `obo:briefcase:briefcase.files.read`.

OBO means **on behalf of**: a service calls a provider for a particular user and selected organization. It does not turn an application credential into unrestricted user authority. Both service configuration and current user authorization matter.

## Expose your own endpoints
```json
{
  "base_url": "https://api.example.com",
  "obo_endpoints": [{
    "endpoint_id": "my-app.documents.read",
    "path": "/api/v1/documents",
    "metadata": {},
    "critical": true,
    "ttl_seconds": 300,
    "enabled": true
  }]
}
```
Endpoint IDs must be unique, nonempty, at most 128 characters, and use letters, digits, dots, underscores, or hyphens. Paths must be absolute endpoint paths without traversal, query, or fragment. TTL must be positive. Exposing endpoints requires a backend `base_url` origin.

The receiving service must verify IAM's delegated authorization, audience, endpoint, organization context, and relevant permissions. Registering an endpoint does not implement its handler or authorization checks for you.

## After an approval changes
Reconcile accepted application state, then renew affected user consent/session grants. A cached session may lack a new scope even though the management API reports that scope as effective. [Authentication](/authentication/) and [webhook management](/webhooks/) cover those transitions.

## List the user's organization apps

Honeycomb exposes the **critical** OBO endpoint `honeycomb.apps.list` at
`POST /api/v1/obo/apps/list`. It returns every catalog app owned by the selected
organization, including private, pending, and disabled entries. State is included
so an inventory entry is not mistaken for permission to install or log in.
Applications from other organizations are excluded, even when public.

Declare access through Honeycomb:

```json
{"app_scope":{"external":[{"app_id":"honeycomb","endpoint_id":"honeycomb.apps.list"}]}}
```

Public callers need the provider's critical-scope approval; private callers follow
IAM's private-app exemption. Both require effective declared access, current user
consent, and current membership in the requested organization. Application
credentials alone cannot authorize an organization inventory.

Use IAM's endpoint catalog and signed OBO exchange with audience `honeycomb`,
endpoint ID `honeycomb.apps.list`, metadata `{}`, and method `POST`. Hash the exact
JSON bytes that will be sent. For example:

```http
POST /api/v1/obo/apps/list
Content-Type: application/json
X-IAM-OBO-Access-Proof: <single-use proof>

{"org_id":"tos","limit":100}
```

The response is deliberately limited to catalog identity and status:

```json
{"org_id":"tos","items":[{"app_id":"internal","org_id":"tos","name":"Internal","description":"Organization application","visibility":"private","state":"active"}],"next_cursor":null}
```

`limit` defaults to 100 and accepts 1–100. If `next_cursor` is present, send its
value as `after` in the next request, with a new proof bound to that request body.
Rows are ordered by `app_id`. Configuration, webhook secrets, app credentials,
testing keys, and unpublished configuration details are never returned.
Honeycomb verifies the proof with IAM on each request, binding method, path, and
body digest; IAM checks live authority and consumes the proof. A reused, expired,
revoked, or mismatched proof is rejected. Proof verification does not disclose a
user's org role unless IAM authorizes that separate scope, and listing does not
require an admin role or `self.identity.read` disclosure.

For a test environment, pass `X-Testing-Environment-Key`. Honeycomb resolves only
that ready environment, recovers its own recipient credential through the
protected IAM integration, and verifies the proof there. The proof's environment
must match; invalid keys, cleanup, and mismatched credentials never fall back to
production. This selects an isolated testing environment, independently of a
package's dev/prod release channel.

Users with a Honeycomb login can request the same inventory through
`GET /api/v1/organizations/{org}/apps?limit=100&after=...`, authenticated with their
Honeycomb bearer token. Current members, including Silicon users, have access;
other organizations and anonymous callers do not.

### Registering this provider endpoint

The backend advertises the definition through `/api/contracts` under
`obo_endpoints`. For deployment, merge the definition from
`deploy/honeycomb-obo.json` into Honeycomb's existing application configuration,
then submit it through Honeycomb's normal configuration/IAM reconciliation flow.
Preserve existing endpoint definitions and configuration. The definition is
critical, enabled, and uses a 60-second proof TTL. Shipping the HTTP handler alone
does not register its scope in a running IAM instance. Each existing testing
import must be explicitly refreshed to receive the new definition; production
configuration does not silently change test snapshots.
