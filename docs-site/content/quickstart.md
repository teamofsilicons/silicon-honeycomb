## 1. Install the CLI
On macOS or Linux:
```sh
printf "Starting Honeycomb installer…\n"; /bin/bash -c "$(curl -fL --progress-bar --connect-timeout 20 --max-time 120 https://raw.githubusercontent.com/teamofsilicons/silicon-honeycomb/main/install.sh)"
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
Open the returned IAM login URL and choose one Carbon or Silicon account together with one organization. IAM asks for consent when Honeycomb requests critical IAM permissions. Briefcase storage access is approved separately when you need it. Supply the one-use, short-lived login token to Honeycomb:
```sh
honeycomb login '<IAM short-lived token>'
honeycomb login status --json
```
Replace the placeholder; do not literally paste it. A token can be exchanged once and expires quickly. Keep it out of shared logs and screenshots. [Authentication and consent](/authentication/) explains expired tokens and refreshed permissions.

## 3. Discover and install
```sh
honeycomb search
honeycomb search briefcase
honeycomb apps get 'my-app'
honeycomb releases list 'my-app'
honeycomb install 'my-app'
honeycomb installed
```
Replace `my-app` with the bare application ID shown in your catalog. The owning organization is separate from the app ID. An empty search result does not mean your CLI installation failed. Quote development selectors such as `'my-app>test'`: an unquoted `>` is shell redirection.

## 4. Keep it current
```sh
honeycomb update 'my-app'
honeycomb uninstall 'my-app'
honeycomb logout
```
Ready to distribute your own app? Continue to [Upload an application](/upload-an-app/).
