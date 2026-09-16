## 1. Install the CLI
On macOS or Linux:
```sh
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/teamofsilicons/silicon-honeycomb/main/install.sh)"
```
The installer selects your native binary, verifies its SHA-256, and configures supported shell startup files. Open a new terminal, then check:
```sh
honeycomb --version
honeycomb --help
```
To activate the default installation in the terminal you already have open:
```sh
source "$HOME/.honeycomb/dir/env"
```
For Windows or a Cargo-based installation, see [Install the CLI](/installation/).

## 2. Sign in when you need private access
```sh
honeycomb iam --json
```
Open the returned IAM login URL. Review Honeycomb's permissions and choose the organizations you want to share. Supply the one-use, short-lived token to Honeycomb:
```sh
honeycomb login '<IAM short-lived token>'
honeycomb login status --json
```
Replace the placeholder; do not literally paste it. A token can be exchanged once and expires quickly. Keep it out of shared logs and screenshots. [Authentication and consent](/authentication/) explains expired tokens and refreshed permissions.

## 3. Discover and install
```sh
honeycomb search
honeycomb search briefcase
honeycomb apps get 'my-org>my-app'
honeycomb releases list 'my-org>my-app'
honeycomb install 'my-org>my-app'
honeycomb installed
```
Replace `my-org>my-app` with an application shown in your catalog that has a release. An empty search result does not mean your CLI installation failed. Always quote application identifiers: an unquoted `>` is shell redirection.

## 4. Keep it current
```sh
honeycomb update 'my-org>my-app'
honeycomb uninstall 'my-org>my-app'
honeycomb logout
```
Ready to distribute your own app? Continue to [Upload an application](/upload-an-app/).
