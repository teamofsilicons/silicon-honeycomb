## Two independent secrets
The application secret authenticates your app to IAM. The webhook signing secret lets your receiver validate events. Rotate them through their respective workflows. Configure your receiver to verify IAM's signed delivery according to the IAM contract and reject invalid/replayed requests.

## Inspect and approve a destination
```sh
honeycomb apps get 'my-org>my-app'
honeycomb apps webhook status 'my-org>my-app'
honeycomb apps webhook approve 'my-org>my-app' --endpoint PENDING_ENDPOINT_UUID --revision 1 --step-up-file /secure/path/assertion
```
Use the exact pending endpoint identifier from status. Obtain a fresh IAM verified-channel assertion bound to the returned resource and `application.webhook.approve`. A normal login token is not a step-up assertion.

IAM's current accepted application record omits the active webhook URL. The console therefore labels the configured URL as requested; do not treat it as authoritative confirmation of the active receiver.

## Rotate webhook signing material
```sh
honeycomb apps webhook rotate-secret 'my-org>my-app' --revision 1 --secret-file /secure/path/new-signing-secret --step-up-file /secure/path/assertion
honeycomb apps webhook retry OPERATION_ID --step-up-file /secure/path/fresh-assertion
```
Signing material must contain 32–4096 bytes without control characters. Rotation uses `application.webhook_secret.rotate` step-up authority. Keep files private. After acceptance, update your receiver's configuration according to IAM's overlap policy. For a pending operation, retry the existing operation ID; you do not need to resubmit its signing secret.

## Rotate the application secret
```sh
honeycomb --idempotency-key my-secret-rotation-001 apps rotate-secret 'my-org>my-app' --revision 1
```
If IAM requests step-up, repeat the original logical request with its original idempotency key and a fresh assertion:
```sh
honeycomb --idempotency-key my-secret-rotation-001 apps rotate-secret 'my-org>my-app' --revision 1 --step-up-file /secure/path/assertion
```
Save the one-time result securely and update your server-side secret store. IAM controls revocation and credential versions. Do not assume an old credential remains usable.

## Production and testing
These management routes currently support production applications. Test-only application administration and some webhook metadata remain integration gaps.
