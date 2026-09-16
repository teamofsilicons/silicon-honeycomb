## Inspect and choose a home
```sh
honeycomb config show
honeycomb config home /existing/directory
honeycomb config env
```
Data lives below `<home>/.honeycomb/dir`. `SILICON_HOME` overrides the initial base home; `config home` saves a pointer to an existing directory. Sessions, package records, and launchers are separated by API origin and testing context.

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
