## macOS and Linux
```sh
printf "Starting Honeycomb installer…\n"; /bin/bash -c "$(curl -fL --progress-bar --connect-timeout 20 --max-time 120 https://raw.githubusercontent.com/teamofsilicons/silicon-honeycomb/main/install.sh)"
```
Requires Bash, curl, tar, and either `sha256sum` or `shasum`. Prebuilt binaries support x86_64 and aarch64 on macOS and Linux. The installer checks the archive checksum, requires exactly one regular `honeycomb` executable, and activates the replacement atomically.

The installer reports preparation immediately, then download progress, checksum verification, activation, and shell/update-worker setup. Source installations also show Cargo’s build progress.

The default binary lives in `~/.honeycomb/dir/system/bin`. Bash, Zsh (including `ZDOTDIR`), and supported POSIX shell startup files receive an idempotent PATH entry. A child installer cannot change the parent terminal's environment: start a new terminal or run the printed activation command.

## Cargo and Windows
Install Rust and its platform build prerequisites, then run:
```sh
cargo install silicon-honeycomb-cli --version '=0.2.2' --locked
honeycomb --version
honeycomb service install
```
The command is `honeycomb`; the crate is `silicon-honeycomb-cli`. Cargo places the binary in its configured bin directory, usually `~/.cargo/bin`; make sure that directory is on PATH. Cargo installation supports Windows, Linux, and macOS. Native release downloads also include Windows x86_64 and aarch64.

[Release archives and checksums](https://github.com/teamofsilicons/silicon-honeycomb/releases/tag/v0.2.2) are available for all six required targets. The Bash installer itself is for macOS/Linux.

## Choose a version or installation home
```sh
printf "Starting Honeycomb installer…\n"; HONEYCOMB_VERSION=0.2.2 /bin/bash -c "$(curl -fL --progress-bar --connect-timeout 20 --max-time 120 https://raw.githubusercontent.com/teamofsilicons/silicon-honeycomb/main/install.sh)"
```
| Variable | Effect |
| --- | --- |
| `SILICON_HOME` | Base directory; Honeycomb data lives under `.honeycomb/dir` inside it. |
| `HONEYCOMB_VERSION` | Select a GitHub release version without the `v` prefix. |
| `HONEYCOMB_NO_MODIFY_PATH=1` | Leave shell startup files alone. |
| `HONEYCOMB_NO_SERVICE=1` | Skip automatic registration of the background update worker. |
| `HONEYCOMB_SOURCE_DIR` | Explicit local checkout for a Cargo source installation. |
| `HONEYCOMB_RELEASE_BASE` | Advanced override for the release source; HTTPS is required outside explicit loopback testing. |

## Update Honeycomb itself
```sh
honeycomb self-update
honeycomb config set auto_update false
honeycomb daemon --once
```
The per-user worker checks at second `01` of every minute and honors `auto_update`. It uses launchd on macOS, systemd user services on Linux, and the platform task mechanism on Windows. Self-update verifies vendor release integrity. The worker also updates installed applications in their original account and environment context. CLI commands check for due updates too. Use `honeycomb update APP_ID` for an immediate application update.

Interactive `install`, `update`, and `self-update` commands show progress on stderr from startup through release lookup, download, verification, and installation. Download percentages reflect bytes received, not estimated total installation time. Redirected output uses plain progress lines; `--json` and background maintenance omit progress output.

## Build from source
```sh
git clone https://github.com/teamofsilicons/silicon-honeycomb.git
cd silicon-honeycomb
cargo install --path crates/cli --locked
```
The workspace currently requires Rust 1.98 or newer. [Troubleshooting](/troubleshooting/) covers PATH and network errors.

## Latest Honeycomb CLI release
`GET https://backend.honeycomb.teamofsilicons.com/api/v1/cli/latest` returns the latest published stable Honeycomb CLI version, release page, and archive/checksum URLs for all six platforms. No login is required. Discovery uses GitHub’s latest release and requires all native assets to be uploaded; drafts, prereleases and incomplete releases are never advertised. Results may be cached for up to one minute; HTTP responses advertise only the remaining cache lifetime so downstream caches do not extend that window.

`honeycomb self-update` and the minute CLI worker use this endpoint, then verify the downloaded checksum and executable version before activation. This endpoint is for Honeycomb itself, independent of application releases.
