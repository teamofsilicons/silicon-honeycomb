## macOS and Linux
```sh
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/teamofsilicons/silicon-honeycomb/main/install.sh)"
```
Requires Bash, curl, tar, and either `sha256sum` or `shasum`. Prebuilt binaries support x86_64 and aarch64 on macOS and Linux. The installer checks the archive checksum, requires exactly one regular `honeycomb` executable, and activates the replacement atomically.

The default binary lives in `~/.honeycomb/dir/system/bin`. Bash, Zsh (including `ZDOTDIR`), and supported POSIX shell startup files receive an idempotent PATH entry. A child installer cannot change the parent terminal's environment: start a new terminal or run the printed activation command.

## Cargo and Windows
Install Rust and its platform build prerequisites, then run:
```sh
cargo install silicon-honeycomb-cli --version '=0.1.0' --locked
honeycomb --version
honeycomb service install
```
The command is `honeycomb`; the crate is `silicon-honeycomb-cli`. Cargo places the binary in its configured bin directory, usually `~/.cargo/bin`; make sure that directory is on PATH. Cargo installation supports Windows, Linux, and macOS. Native release downloads also include Windows x86_64 and aarch64.

[Release archives and checksums](https://github.com/teamofsilicons/silicon-honeycomb/releases/tag/v0.1.0) are available for all six required targets. The Bash installer itself is for macOS/Linux.

## Choose a version or installation home
```sh
HONEYCOMB_VERSION=0.1.0 /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/teamofsilicons/silicon-honeycomb/main/install.sh)"
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
The per-user worker checks hourly and honors `auto_update`. It uses launchd on macOS, systemd user services on Linux, and the platform task mechanism on Windows. Self-update verifies vendor release integrity. Application updates use `honeycomb update APP_ID` separately.

## Build from source
```sh
git clone https://github.com/teamofsilicons/silicon-honeycomb.git
cd silicon-honeycomb
cargo install --path crates/cli --locked
```
The workspace currently requires Rust 1.98 or newer. [Troubleshooting](/troubleshooting/) covers PATH and network errors.
