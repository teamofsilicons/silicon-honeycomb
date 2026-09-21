# Session renewal

The Honeycomb CLI, `login status` and background package updater share one session
file per API origin and testing context. Each reader locks a separate `session.lock`
file before loading credentials. Refresh saves its idempotency key and request start
time before contacting Honeycomb, then atomically replaces credentials after a valid
response. A retry from another command or process reuses that saved operation.
Malformed replies and temporary network or service failures retain the prior session
and retry key. Access expiry is measured from the original request start, including
replayed responses; a replay that is already expired triggers a fresh rotation using
its saved successor token.

Login and logout use the same lock. Logout revokes the saved family before clearing
the local file, preventing a concurrent refresh from writing credentials back after
sign-out. Test and production sessions remain in separate context directories.
Existing session files are compatible; no re-login or local migration is required.

The library and console use the same rules with encrypted persistent SQLite rows.
They also retry one rejected access token after renewal, preserving the original
business request and its mutation idempotency key. Refresh retries are recoverable
within IAM's idempotency replay window; a revoked family or an uncertain operation
older than that window may still require a new login.

Validation: `cargo test -p silicon-honeycomb-cli`,
`cargo clippy -p silicon-honeycomb-cli --all-targets -- -D warnings`,
`cd web && npm run test:unit && npm run build`, and the Playwright public-login and
concurrent-refresh journeys. Tests use isolated temporary homes and databases with
fixture credentials, including real concurrent CLI and daemon processes.
