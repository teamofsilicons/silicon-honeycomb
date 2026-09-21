## Manage a bundle

A bundle presents one sign-in identity for several applications in the same organization. Honeycomb manages its name, logo and members; IAM checks authority, accepts configuration revisions and issues separate credentials after each application's consent checks. A bundle does not have a CLI archive or an application secret of its own.

Open **Bundles** in the developer console, select an existing bundle or choose **Create bundle**, and enter its organization-qualified ID, name and member application IDs. Only current organization owners or admins can manage bundles. IAM additionally requires an active, trusted organization with bundled applications enabled. Membership changes do not grant additional permissions or bypass consent.

The deployed console supports these operations now. The following CLI commands require a build containing bundle support; they are not included in the public 0.2.2 binary release:

```sh
honeycomb bundles list
honeycomb bundles get 'tos>interface'
honeycomb bundles configure 'tos>interface' interface-bundle.json --revision 1
```

Read the current `iam_revision` first. Use revision zero only when creating a new ID. Existing IAM bundles appear automatically and are configured in place; their immutable identity is preserved. A configuration file contains `app_name`, optional `app_logo`, and `app_ids` (1–100 unique applications belonging to the same organization).

Every mutation requires an idempotency key and current IAM revision. The CLI generates a key unless `--idempotency-key` is supplied. Supply an explicit key when you need to retry across invocations; retry the exact request with that same key after a transport failure. Honeycomb durably saves the desired definition and operation before contacting IAM, prevents overlapping pending writes, and reports success only for a matching accepted receipt. Read state reflects IAM's current accepted configuration. Test credentials cannot manage production bundles.

HTTP clients use `GET /api/v1/bundles`, `GET /api/v1/bundles/{bundle_id}`, and `PUT /api/v1/bundles/{bundle_id}`. Writes require `If-Match: <iam_revision>` and `Idempotency-Key`. These routes require an authenticated Honeycomb session; the management service credential alone is never a user's authority.
