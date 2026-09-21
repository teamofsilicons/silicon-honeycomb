## Upload a release
```sh
honeycomb apps get 'my-org>my-app' --json
honeycomb --idempotency-key my-release-upload-0001 releases upload 'my-org>my-app' my-app-1.0.0.tar.gz --channel prod --revision 1
honeycomb releases list 'my-org>my-app'
```
Use the application's current configuration revision. Run `honeycomb validate ARCHIVE` for local validation before uploading. The upload uses the authenticated server validator, which retains validation failures for publication troubleshooting. It uses the selected application's identity; an optional manifest `app_id` must match it. Briefcase stores the bytes, while Honeycomb records version, SHA-256, size, and creation time.

A three-part numeric version (`x.y.z`) identifies immutable release bytes within a channel. Explicitly choose `--channel prod` or `--channel dev` on upload. Each channel has independent history and may use the same version number. Existing releases belong to production. Publish `1.0.1` for changed bytes rather than replacing `1.0.0` in that channel. Clients verify the recorded hash, byte length, manifest identity, and version before installation.

The current CLI, Rust client, and console use `/api/v2/apps/{app_id}/releases` for channel-aware release lists and uploads, `/api/v2/apps/{app_id}/download` for downloads, and `/api/v2/apps/{app_id}/releases/{dev_version}/promote` for promotion. V2 uploads require `?channel=prod` or `?channel=dev`. If sending the optional `Honeycomb-API-Version` header, set it to `v2`. Existing v1 uploads without a channel continue to target production. Application configuration, authentication, and publication APIs remain on v1; see [Compatibility](/compatibility/).

## Development releases and promotion
```sh
honeycomb releases list 'my-org>my-app' --channel dev
honeycomb releases promote 'my-org>my-app' 1.0.0 --version 2.0.0 --revision 1
honeycomb install 'my-org>my-app>test@1.0.0'
honeycomb install 'my-org>my-app@2.0.0' --switch-channel
```
Promotion creates a new production release with the selected version while preserving the original dev release. The manifest version and archive checksum are updated; packaged executable bytes are preserved. Production listings and normal installation defaults never select a dev release. Switching an existing install prompts for confirmation; unattended calls require `--switch-channel`. Use distinct `--alias` values to keep both installed. Automatic updates follow each installed channel independently.

Promotion does not rebuild executables. A command's compiled `--version` output may still show the original development version, while `honeycomb installed` reports the chosen production package version. If the executable must report the production version itself, build it with that version and upload a new production archive.

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
