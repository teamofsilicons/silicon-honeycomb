## “honeycomb: command not found”
The installer configures future shells. Open a new terminal or, for the default native installation, run:
```sh
source "$HOME/.honeycomb/dir/env"
honeycomb --version
```
Cargo installations need Cargo's bin directory on PATH. Custom homes need the path printed by the installer. Check `honeycomb config env` after changing home or backend. Re-running the native installer does not duplicate its shell entry.

If Honeycomb works but an installed app reports `command not found`, run `honeycomb install 'app'` again. App installs now repair persistent PATH setup even when the package is already present. Open a new terminal or run the activation command printed at completion. A `path_setup.status` of `manual` means startup-file configuration failed; its message identifies the error. `HONEYCOMB_NO_MODIFY_PATH=1` and testing environments intentionally skip persistent setup.

## The install command is quiet
The install command prints a starting message before fetching `install.sh`, and curl shows transfer progress. The fetch has a 20-second connection timeout and a two-minute overall timeout. The script then reports preparation, binary download, verification, activation, and setup. If the initial fetch fails, check access to GitHub/raw.githubusercontent.com and retry.

## IAM token expired or was already used
Request a fresh Honeycomb token and exchange it promptly, once. Use the URL returned by `honeycomb iam`. A token issued for another application cannot sign you into Honeycomb. Authentication rejection now returns a sign-in recovery error instead of being reported as a generic service outage.

## Create application is disabled
You need a current owner/admin role in a shared organization and effective membership disclosure. Verify `self.membership.read`, renew IAM consent by signing in again, and check that you selected the right organization. `honeycomb login status --json` shows the CLI's current live authority; the website has its own session.

## “Missing required target” or archive mismatch
Build all six required native targets, populate their executable mappings, and make non-Windows files executable. Keep `honeycomb.yaml` at the archive root. If `app_id` is present, ensure it exactly matches the application's permanent identifier; otherwise omit it. Run `honeycomb validate` before upload. See [Package format](/package-format/).

## Upload or private download fails with delegated access errors
Check the platform's four Briefcase OBO grants, `self.tags.read`, membership/identity disclosure, current user consent, and the resource organization's selection. Effective management scopes alone do not renew an existing token. An IAM/Briefcase integration problem cannot be fixed by repeatedly creating the app.

## App exists but its first release is missing
Registration may succeed before the archive upload fails. Reopen that application's **Releases** tab and retry there. Check operations and existing versions first. Keep the original idempotency key for an uncertain CLI upload; choose a new semantic version only when the package content changes.

## Revision conflict
Read the latest app, review, draft, or environment record. Review concurrent edits before constructing a new logical mutation with its new revision and key. Never change the body or revision under an already-used idempotency key.

## Command collision during installation
Use `--alias original=new-name`. Honeycomb will not replace a command owned by another package in its own bin directory. Update preserves recorded aliases.

## A command on PATH is older than the package needs
Honeycomb reads the version already serving that command and reports what it found: `You have dm 0.7.0 on your system at /path/to/dm; this package needs dm 0.9.2 to continue`. Answer the prompt to rewrite that command where it sits; uninstall restores the file it displaced. A command already new enough is reported as resolved and left alone. Unattended runs and `--json` cannot be prompted, so they stop: pass `--rewrite-existing` to accept the rewrite in advance, or `--alias` to install alongside.

## Pending forever
Inspect `honeycomb operations get OPERATION_ID` or the environment's service progress. Fix the named integration before retrying. Some lifecycle/publication contracts remain incomplete. Health checks and local tests do not turn a pending remote operation into an accepted one.

## Need help
Read command-specific `--help`, inspect the [CLI reference](/cli-reference/), and submit a concise reproduction through `honeycomb report`. Include versions and safe error codes, not tokens, app secrets, testing keys, or sensitive package bytes.
