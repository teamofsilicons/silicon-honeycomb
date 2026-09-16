## Requested versus effective
`app_scope` describes what your app requests. IAM decides what is accepted and currently effective. A user must also consent to the requested account/organization data. Honeycomb saves requested configuration and displays accepted state separately.

## Request access to another application
```json
{
  "app_scope": {
    "iam": ["self.identity.read", "self.membership.read", "self.tags.read"],
    "external": [
      {"app_id": "tos>briefcase", "endpoint_id": "briefcase.files.read"}
    ]
  }
}
```
This is a configuration fragment, not a full creation document. Choose only what your app needs. External endpoint IDs come from the provider's registered catalog. For Briefcase, the delegated effective scope is represented as `obo:tos>briefcase:briefcase.files.read`.

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
