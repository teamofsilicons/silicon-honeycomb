## Inspect retention
```sh
honeycomb environments retention ENVIRONMENT_ID
honeycomb environments set-retention ENVIRONMENT_ID --days 60 --revision 1
```
The environment view reports activity, deadlines, and dependency protection without refreshing activity merely because someone opened the page. Changes require current authority, the environment revision, and no conflicting pending lifecycle operation.

Each application has `testing_idle_days` (default 30, range 1–36500). Imported apps use their production organization's accepted policy. Longer application retention can extend the shared environment deadline.

## Report real application use
```sh
honeycomb --test ENVIRONMENT_ID environments activity ENVIRONMENT_ID 'my-org>my-app' --generation 1 --key-version 1
```
Use the actual current generation and key version from environment state. A current environment manager can also report using authenticated authority. Reports cannot create application links or cross environment boundaries.

Report a real operation, not a periodic heartbeat designed to keep an unused environment alive. The server records its own time. Idempotent replay returns the original timestamp instead of extending the inactivity window.

## Dependency protection
An idle provider remains protected while an active application depends on it, including transitive dependencies and cycles. Retirement removes only selected applications and their owned test data. Active apps and required dependencies remain available.

The worker records cleanup jobs durably, retries incomplete participant work with backoff, and exposes progress. Soft deletion's recovery clock begins after disabled-access receipts from every participant. Cleanup itself does not refresh application activity.

## Current limitation
The automatic-retention participant contract is not fully mapped by the current IAM adapter. Jobs must stay visibly pending until the service confirms the action. Keep scheduled-testing cutovers disabled until these cross-service behaviors are accepted. [Availability](/availability/) tracks this limitation separately from local tests.
