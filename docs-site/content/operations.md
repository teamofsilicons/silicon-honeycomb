## Several versions, different meanings
| Value | Changes when |
| --- | --- |
| Release version (`1.0.0`) | You publish new package bytes. |
| Application revision | Honeycomb configuration changes. |
| IAM revision | IAM's accepted application state changes. |
| Review/environment revision | That workflow record changes. |
| API major (`v1`) | The HTTP contract changes incompatibly. |

Read current state before a mutation. The API sends revision preconditions with `If-Match`; CLI commands expose `--revision`. A stale revision is a conflict, not permission to overwrite newer work.

## Persist the idempotency key
```sh
honeycomb --idempotency-key my-config-change-0001 apps update 'my-org>my-app' application.json --revision 2
```
Generate a unique key per logical mutation and record it before sending. If the response is uncertain, retry the same key and exact input. Do not change the revision or body under that key. A fresh action needs a fresh key.

## Inspect pending work
```sh
honeycomb operations get OPERATION_ID
honeycomb operations retry OPERATION_ID
honeycomb apps reconcile 'my-org>my-app'
```
Honeycomb stores durable operations around cross-service work. A pending result can mean IAM acceptance or storage reconciliation is still outstanding. Inspect the status and repair the reported integration before retrying. A successful HTTP response that says pending is not completed activation.

## Credential recovery
```sh
honeycomb operations recover-secret OPERATION_ID
```
Creation and rotation secrets are returned through explicit credential-returning workflows. IAM controls their short recovery/replay window. Honeycomb does not save a plaintext result for unlimited recovery. If the window has expired, use an authorized secret rotation, not a duplicate creation.

## Reconciliation and notifications
IAM's signed management notifications identify accepted revision changes. Honeycomb deduplicates receipts and reconciles authorized current state. Event receipt alone does not prove that an application was adopted or that all downstream work completed. Administrators can explicitly reconcile through the console or CLI.
