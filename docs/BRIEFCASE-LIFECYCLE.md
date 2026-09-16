# Briefcase lifecycle participant

The management adapter sends Briefcase operations to
`PUT /internal/honeycomb/organizations/{org_id}/testing-environments/{environment_id}/operations/{operation_id}`.
The coordinator's JSON operation is sent unchanged. Retries reuse the same operation ID and body;
Briefcase stores pending, completed and failed receipts. Honeycomb accepts completion only for
the exact environment, revision, generation and key version, plus the retirement selection.

Managed AWS deployment automatically provisions and verifies the shared lifecycle
credential in both services' secret stores. No app owner needs to generate, copy,
or configure a token when registering an app or creating a testing environment.
Existing credentials are reused across deployments. The running services load
`BRIEFCASE_HONEYCOMB_SERVICE_TOKEN` internally; it remains independent of application
secrets, testing keys and user sessions. Honeycomb's management adapter also uses
its existing `IAM_HONEYCOMB_SERVICE_CREDENTIAL` and HTTPS `BRIEFCASE_BASE_URL`.

For self-hosted deployments, provision a dedicated shared token of at least 32
visible ASCII characters through the deployment secret store. Normal and automatic
retention operations use service authentication; user credentials are not forwarded.

The transport rejects redirects, limits each request to 30 seconds (5 seconds to connect),
and limits receipts to 64 KiB. Failed requests remain retryable through the coordinator.

This wiring does not implement IAM's shared-environment lifecycle transport. That existing
adapter remains unavailable until its separate contract is implemented; environments requiring
IAM cannot become ready merely by enabling Briefcase's participant.
