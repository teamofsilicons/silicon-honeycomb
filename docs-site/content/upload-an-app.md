## Before you begin
You need a shared IAM organization in which you are an owner/admin, a unique application handle, a real HTTPS webhook receiver and its signing secret, and native CLI builds for all six required targets. Honeycomb packages binaries; it does not compile your application.

## 1. Prepare the package
Create a directory with a root `honeycomb.yaml` and the six platform payloads under `targets/`. Every target must map every declared command to a nonempty executable. Use the [manifest reference](/package-format/) and [downloadable manifest](/examples/honeycomb.yaml).
```sh
honeycomb validate ./my-app
honeycomb pack ./my-app --output my-app-1.0.0.tar.gz
honeycomb validate ./my-app-1.0.0.tar.gz
```
When the manifest is missing, `pack` creates a starter template and asks you to populate it. It does not manufacture missing native builds. Use the version in your manifest to name the archive.

## 2. Register in the console
1. Open the [console](https://console.honeycomb.teamofsilicons.com) and sign in.
2. Choose **Create application** and the owning organization.
3. Set the permanent handle, display name, and 50–1,000 word description.
4. Under **First CLI release**, choose **Production** or **Development**, then choose your `.tar.gz`.
5. Configure your HTTPS webhook URL, signing secret, and needed scopes.
6. Leave **Publication preference** set to public, or explicitly choose private, then submit **Create application** and securely save any one-time application secret.

The console validates the archive before registration, including its application ID. After registration it opens **Releases** and uploads the selected archive. If upload fails after registration, retry the upload for that existing application; do not create a second app. Text fields can be saved as a shared draft, but files and signing secrets must be supplied again after reopening it.

You may leave the first release empty to create configuration first. A valid six-target production release is required before publication; development releases can be promoted when ready.

## 3. Or register and upload through the CLI
Prepare a private `application.json` using the [application configuration reference](/application-config/). Then:
```sh
honeycomb --idempotency-key my-app-create-0001 apps create application.json
honeycomb apps get 'my-org>my-app' --json
```
Wait for IAM acceptance if creation returns a pending operation. Read the returned application revision; use that current number below instead of assuming it is always 1.
```sh
honeycomb --idempotency-key my-app-upload-0001 releases upload 'my-org>my-app' my-app-1.0.0.tar.gz --channel prod --revision 1
honeycomb releases list 'my-org>my-app'
```
Every upload requires an explicit `--channel prod` or `--channel dev`. Versions must use three numeric components, such as `2.4.1`; prerelease labels and build metadata are not accepted. Reuse the same idempotency key, revision, channel, and archive if the response was lost. A new version within that channel is required for different release bytes. The same version number may exist independently in both channels.

## 4. Verify private installation
While signed in with access to the owning organization:
```sh
honeycomb install 'my-org>my-app@1.0.0'
honeycomb installed
```
Run your installed command's help and a representative operation. Test native builds on their actual operating systems; archive shape validation alone does not prove a binary runs.

## Development releases and promotion
Development releases use an independent version history. Upload an experimental build and install it with `>test`:
```sh
honeycomb releases upload 'my-org>my-app' my-app-2.4.1.tar.gz --channel dev --revision 1
honeycomb releases list 'my-org>my-app' --channel dev
honeycomb install 'my-org>my-app>test@2.4.1'
```
When that build is ready, choose a new production version:
```sh
honeycomb releases promote 'my-org>my-app' 2.4.1 --version 1.1.0 --revision 1
```
Use the current application revision. Promotion creates a production archive with the chosen version and preserves the development release. In the console, open **Releases & publication**, select **Development** under **Release history channel**, choose **Promote … to production**, and enter the production version. The same screen lets you upload subsequent archives with an explicit channel.

## 5. Follow public approval
New production applications request public approval automatically after IAM accepts the configuration and the first valid production release is uploaded or promoted from development. Set `"visibility": "private"` in the application input or select private in the console to opt out. Until all required approvals pass, the application remains private. Existing applications retain their stored preference.

Follow [Publication and reviews](/publication/). Private registration, successful storage, and public approval are separate states. If your archive or logo upload reports missing delegated permissions, use [Troubleshooting](/troubleshooting/); the platform's Briefcase integration must be configured first.

Publication errors list each unmet prerequisite separately. If the latest upload for the current application revision failed validation, the response includes the exact validator errors, for example `targets.windows-aarch64: required 64-bit target is missing`. The CLI upload uses the authenticated server validator so those errors remain available to later publication requests. `honeycomb validate` still works locally before uploading. If no upload reached the server, Honeycomb reports that no release has been uploaded successfully; it does not invent a validation result. IAM activation details appear only while activation is pending. A successful upload clears the saved failure, and failures from older application revisions are not shown.
