## “honeycomb: command not found”
The installer configures future shells. Open a new terminal or, for the default native installation, run:
```sh
source "$HOME/.honeycomb/dir/env"
honeycomb --version
```
Cargo installations need Cargo's bin directory on PATH. Custom homes need the path printed by the installer. Check `honeycomb config env` after changing home or backend. Re-running the native installer does not duplicate its shell entry.

## The install command is quiet
The outer `curl -fsSL` first fetches `install.sh`; it is intentionally quiet. Once the script starts it prints progress. Check access to GitHub/raw.githubusercontent.com if that initial fetch stalls. Downloading the script separately with curl's timeout/progress options can distinguish a network issue from installation work.

## IAM token expired or was already used
Request a fresh Honeycomb token and exchange it promptly, once. Use the URL returned by `honeycomb iam`. A token issued for another application cannot sign you into Honeycomb. Authentication rejection now returns a sign-in recovery error instead of being reported as a generic service outage.

## Create application is disabled
You need a current owner/admin role in a shared organization and effective membership disclosure. Verify `self.membership.read`, renew IAM consent by signing in again, and check that you selected the right organization. `honeycomb login status --json` shows the CLI's current live authority; the website has its own session.

## “Missing required target” or archive mismatch
Build all six required native targets, populate their executable mappings, and make non-Windows files executable. Keep `honeycomb.yaml` at the archive root. Ensure its `app_id` exactly matches the application's permanent identifier. Run `honeycomb validate` before upload. See [Package format](/package-format/).

## Upload or private download fails with delegated access errors
Check the platform's four Briefcase OBO grants, `self.tags.read`, membership/identity disclosure, current user consent, and the resource organization's selection. Effective management scopes alone do not renew an existing token. An IAM/Briefcase integration problem cannot be fixed by repeatedly creating the app.

## App exists but its first release is missing
Registration may succeed before the archive upload fails. Reopen that application's **Releases** tab and retry there. Check operations and existing versions first. Keep the original idempotency key for an uncertain CLI upload; choose a new semantic version only when the package content changes.

## Revision conflict
Read the latest app, review, draft, or environment record. Review concurrent edits before constructing a new logical mutation with its new revision and key. Never change the body or revision under an already-used idempotency key.

## Command collision during installation
Use `--alias original=new-name`. Honeycomb will not replace a command owned by another package or found elsewhere on PATH. Update preserves recorded aliases.

## Pending forever
Inspect `honeycomb operations get OPERATION_ID` or the environment's service progress. Fix the named integration before retrying. Some lifecycle/publication contracts remain incomplete. Health checks and local tests do not turn a pending remote operation into an accepted one.

## Need help
Read command-specific `--help`, inspect the [CLI reference](/cli-reference/), and submit a concise reproduction through `honeycomb report`. Include versions and safe error codes, not tokens, app secrets, testing keys, or sensitive package bytes.
