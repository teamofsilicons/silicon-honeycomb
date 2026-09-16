## Find an application
Open the [library](https://honeycomb.teamofsilicons.com) or run:
```sh
honeycomb search briefcase --page 1
honeycomb apps get 'my-org>my-app'
honeycomb releases list 'my-org>my-app'
```
Public applications can be discovered anonymously. Sign in for private applications permitted by your current shared IAM memberships. Search visibility is an authorization decision, not a substitute for download authorization.

The top-right **Create an app** link takes you to the authenticated [console](https://console.honeycomb.teamofsilicons.com). Library browsing does not require an administrative role; creating applications does.

## Install a chosen version
```sh
honeycomb install 'my-org>my-app' --version 1.0.0
honeycomb installed
honeycomb update 'my-org>my-app'
honeycomb uninstall 'my-org>my-app'
```
Omit `--version` to select the current release. The installer checks the payload and activates only your host platform. It refuses command collisions instead of overwriting another installation.

## Resolve a command collision
```sh
honeycomb install 'my-org>my-app' --alias my-app=my-org-app
```
The left side must be a command declared by the package. Updates preserve aliases. Uninstall removes only files and launchers owned by that installation. [Local configuration](/configuration/) explains storage and PATH contexts.

## Reviews, stars, and reports
```sh
honeycomb star 'my-org>my-app'
honeycomb star 'my-org>my-app' --remove
honeycomb review 'my-org>my-app' --rating 5 --review 'A concise, useful review.'
honeycomb report 'Steps to reproduce the problem and the observed result.' --pr https://github.com/teamofsilicons/silicon-honeycomb/pull/123
```
Reviews and stars require authenticated access to the application. A bug report may optionally reference a relevant pull request; replace the illustrative link with a real one. Do not include credentials or private package data in reports. Delivery of report notifications depends on the configured notification service.
