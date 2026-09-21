## Inspect and choose a home
```sh
honeycomb config show
honeycomb config home /existing/directory
honeycomb config env
```
Data lives below `<home>/.honeycomb/dir`. `SILICON_HOME` overrides the initial base home; `config home` saves a pointer to an existing directory. Sessions, package records, and launchers are separated by API origin and testing context.

Successful `install` and explicit `update` commands automatically register the app command directory for future terminals, including when the package is already installed. Bash, Zsh (`ZDOTDIR` included), POSIX shells, and Fish receive an idempotent startup entry; Windows updates the user PATH. Existing shell configuration is preserved. On Unix, `SILICON_HOME` also scopes the startup files so embedded runtimes do not edit the user's normal shell configuration.

An installer cannot change the PATH of its parent terminal. When activation is needed, the completion output prints a command to run in that terminal; subsequent terminals load the saved setup automatically. JSON results include `path_setup` with the setup status and any activation command. If shell setup fails, the installed package is retained and the output explains how to activate it manually. Re-running `install` retries shell setup.

Set `HONEYCOMB_NO_MODIFY_PATH=1` to opt out. Testing contexts (`--test`) require explicit activation and never change persistent shell setup. Background updates do not edit startup files. On Windows, an isolated `SILICON_HOME` also requires explicit activation.

In Bash/Zsh, apply generated PATH entries with:
```sh
eval "$(honeycomb config env)"
```
This command requires the `honeycomb` binary itself already to be discoverable. For a fresh default native install, source `~/.honeycomb/dir/env` first. A custom home or backend needs its matching PATH entries.

## Backend selection
```sh
honeycomb config set api https://backend.honeycomb.teamofsilicons.com
honeycomb --api http://127.0.0.1:18080 search
```
`--api` and `HONEYCOMB_API_URL` select an explicit origin. HTTPS is required except literal loopback development servers. A different origin gets a separate authentication and installation context.

## Updates and diagnostics

Successful app installs register the shared Honeycomb and app update worker by default. It wakes at second `01` of every minute, even while no commands are being used. Each installed channel follows its own latest release; production and dev can coexist with distinct command aliases. `@x.y.z` selects the initial installed version and does not disable later automatic updates. An app installation reports `auto_update.status`; if supervisor registration fails, it retains the app and prints how to retry. `HONEYCOMB_NO_SERVICE=1` skips automatic supervisor registration for embedded runtimes that already run `honeycomb daemon` themselves.

```sh
honeycomb config set auto_update false
honeycomb config set telemetry false
honeycomb config set auto_update true
```
Set `HONEYCOMB_AUTO_UPDATE=0` (also `false`, `off`, or `no`) to disable automatic CLI and package updates for one process, including `daemon --once`, without changing saved preferences. This is suitable for pinned distributions and fresh homes. An explicit `self-update` remains an explicit update request, but Honeycomb refuses to replace a command symlink; use the package manager that owns it.

Boolean settings accept `true` or `false`. `HONEYCOMB_TELEMETRY=false` overrides diagnostics including scheduled checks. Website telemetry preferences are separate browser settings. See [Privacy and diagnostics](/privacy/).

## Global flags
| Flag | Meaning |
| --- | --- |
| `--api URL` | Choose the backend origin. |
| `--test ID_OR_KEY` | Select a saved environment or its 32-character root key. |
| `--json` | Structured results; errors are written to stderr with a nonzero exit code. |
| `--idempotency-key KEY` | Reuse a 16–255 visible-ASCII-character key for the same uncertain mutation. |
| `--help`, `--version` | Inspect command help or CLI version. |

Credential-returning commands may print secrets even with `--json`. Store their output privately; ordinary structured output is not a secret-redaction guarantee.
