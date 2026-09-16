## Your preference
```sh
honeycomb config set telemetry false
```
The CLI also honors `HONEYCOMB_TELEMETRY=false`, including background checks. In either website, open **Telemetry settings** and turn off **Share usage and diagnostics**. Each website stores its browser preference separately. Rust callers use the returned `client.with_telemetry(false)` instance.

## Event contents
Optional diagnostics use fixed command/page families, route templates, status, elapsed time, success, version, and generated request identifiers. They do not include application names/IDs, search text, request/response bodies, local paths, credentials, or free-form error messages.

The authenticated telemetry endpoint accepts an allowlisted schema with a 4 KiB body limit. Only the backend holds a Space Station ingest key. No ingest key belongs in a browser bundle or CLI configuration. Without a configured exporter key, remote export is inactive.

## Testing data and delivery
Test-context diagnostics remain in local test storage rather than using the production table key. They are bounded per environment and removed by the appropriate cleanup lifecycle. Delivery is best effort and does not block ordinary request handling.

Essential operation/revision records and API-contract lifecycle timestamps remain part of service correctness even when optional diagnostics are disabled. User bug reports are explicit submissions; review their content before sending.

## Documentation website
This documentation site serves public static content. Search runs locally in your browser against a bundled index. The site does not require IAM sign-in or load the Honeycomb analytics client. Hosting providers may still retain their normal request logs.
