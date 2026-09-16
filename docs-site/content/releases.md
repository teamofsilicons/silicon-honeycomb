## Upload a release
```sh
honeycomb apps get 'my-org>my-app' --json
honeycomb --idempotency-key my-release-upload-0001 releases upload 'my-org>my-app' my-app-1.0.0.tar.gz --revision 1
honeycomb releases list 'my-org>my-app'
```
Use the application's current configuration revision. The CLI validates the archive locally; the server validates again and confirms the manifest's `app_id` matches the route. Briefcase stores the bytes, while Honeycomb records version, SHA-256, size, and creation time.

A semantic version identifies immutable release bytes. Publish `1.0.1` for changed bytes rather than replacing `1.0.0`. Clients verify the recorded hash, byte length, manifest identity, and version before installation.

## Retry an uncertain upload
If the network drops after storage succeeds, repeat the same request with the same idempotency key, revision, and file. Reusing a key for different content is an error. The console's in-page retry retains the selected file and original key; after reloading, inspect releases and operations before choosing a new upload.

## Logo files
```sh
honeycomb --idempotency-key my-logo-upload-000001 apps upload-logo my-org ./logo.png
```
Use the returned `logo_url` in your application configuration. PNG, JPEG, and WebP are supported, up to 2 MiB and 2048 × 2048 pixels. Honeycomb removes metadata and converts uploaded logos to PNG.

**Uploaded logos have publicly viewable links, including logos for private applications.** Use a suitable non-sensitive image. Archive access remains governed separately by application visibility and publication state.

## Storage authorization
Private reads, uploads, and publication access changes select the resource's owning organization when calling Briefcase on the user's behalf. The identity provider, service issuer, and resource organization can differ. Missing platform OBO grants or missing current consent stop storage operations; they must not silently fall back to another organization or production testing context.
