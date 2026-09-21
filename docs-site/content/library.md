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
honeycomb install 'my-org>my-app@1.0.0'
honeycomb installed
honeycomb update 'my-org>my-app'
honeycomb uninstall 'my-org>my-app'
```
Omit `@1.0.0` to select the latest production release. The existing `--version 1.0.0` option is also supported. The installer checks the payload and activates only your host platform. It refuses to overwrite a command owned by another package in its own bin directory.

## Try experimental development releases
```sh
honeycomb install 'my-org>my-app>test'
honeycomb install 'my-org>my-app>test@2.4.1'
honeycomb releases list 'my-org>my-app' --channel dev
```
Production and development maintain separate histories, even when their version numbers match. Installing an exact version chooses the initial release; automatic updates continue to follow that installation's channel. Honeycomb's daemon checks installed applications every minute at second `01` and applies newer releases from the corresponding channel.

When you switch an existing installation to development, Honeycomb asks whether you want experimental updates. Switching back with `honeycomb install 'my-org>my-app'` asks whether you want official releases instead. Use `--switch-channel` to explicitly authorize switching in unattended commands. Separate command aliases allow both channels to coexist without overwriting each other's commands.

The library's main install command always uses production. Expand **Experimental development releases** in an application's details for the development command.

## Resolve a command collision
```sh
honeycomb install 'my-org>my-app' --alias my-app=my-org-app
honeycomb install 'my-org>my-app' --rewrite-existing
```
When the same command already exists elsewhere on PATH, Honeycomb compares versions instead of failing. A version new enough for the package is reported as resolved and left untouched. An older one is named against what the package requires and you are asked whether to rewrite it; uninstall restores whatever a rewrite displaced. Use `--rewrite-existing` to answer yes in advance, which is what unattended callers and `--json` need. The left side of `--alias` must be a command declared by the package. Updates preserve aliases. Uninstall removes only files and launchers owned by that installation. [Local configuration](/configuration/) explains storage and PATH contexts.

## Reviews, stars, and reports
```sh
honeycomb star 'my-org>my-app'
honeycomb star 'my-org>my-app' --remove
honeycomb review 'my-org>my-app' --rating 5 --review 'A concise, useful review.'
honeycomb report 'Steps to reproduce the problem and the observed result.' --pr https://github.com/teamofsilicons/silicon-honeycomb/pull/123
```
Reviews and stars require authenticated access to the application. A bug report may optionally reference a relevant pull request; replace the illustrative link with a real one. Do not include credentials or private package data in reports. Delivery of report notifications depends on the configured notification service.
