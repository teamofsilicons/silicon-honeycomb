#!/bin/bash
# Honeycomb installer: prebuilt release + SHA-256, or an explicit local source checkout.
set -euo pipefail
umask 077

fail() { printf 'honeycomb installer: %s\n' "$*" >&2; exit 1; }
printf 'Installing Honeycomb…\n'
honeycomb_home="${SILICON_HOME:-${HOME:?HOME or SILICON_HOME must be set}}"
honeycomb_dir="$honeycomb_home/.honeycomb/dir"
honeycomb_bin="$honeycomb_dir/system/bin"
mkdir -p "$honeycomb_bin"
honeycomb_temp="$(mktemp -d "${TMPDIR:-/tmp}/honeycomb-install.XXXXXXXX")"
trap 'rm -rf "$honeycomb_temp"' EXIT

if [ -n "${HONEYCOMB_SOURCE_DIR:-}" ]; then
  command -v cargo >/dev/null || fail 'Cargo is required for source installation. Install Rust, then retry.'
  [ -f "$HONEYCOMB_SOURCE_DIR/crates/cli/Cargo.toml" ] || fail 'HONEYCOMB_SOURCE_DIR must point to the Honeycomb repository.'
  cargo install --locked --path "$HONEYCOMB_SOURCE_DIR/crates/cli" --root "$honeycomb_dir/system" --bin honeycomb
else
  command -v curl >/dev/null || fail 'curl is required.'
  command -v tar >/dev/null || fail 'tar is required.'
  case "$(uname -s)" in Darwin) honeycomb_os=macos ;; Linux) honeycomb_os=linux ;; *) fail 'Use cargo install silicon-honeycomb-cli on this operating system.' ;; esac
  case "$(uname -m)" in arm64|aarch64) honeycomb_arch=aarch64 ;; x86_64|amd64) honeycomb_arch=x86_64 ;; *) fail 'Unsupported architecture; use cargo install silicon-honeycomb-cli.' ;; esac
  honeycomb_target="$honeycomb_os-$honeycomb_arch"
  if [ -n "${HONEYCOMB_RELEASE_BASE:-}" ]; then
    honeycomb_base="$HONEYCOMB_RELEASE_BASE"
  elif [ -n "${HONEYCOMB_VERSION:-}" ]; then
    honeycomb_base="https://github.com/teamofsilicons/silicon-honeycomb/releases/download/v$HONEYCOMB_VERSION"
  else
    honeycomb_base="https://github.com/teamofsilicons/silicon-honeycomb/releases/latest/download"
  fi
  case "$honeycomb_base" in
    https://*) honeycomb_protocol='=https' ;;
    http://127.0.0.1:*|http://localhost:*) [ "${HONEYCOMB_ALLOW_LOCAL:-0}" = 1 ] || fail 'Local HTTP release testing requires HONEYCOMB_ALLOW_LOCAL=1.'; honeycomb_protocol='=http' ;;
    *) fail 'Release URL must use HTTPS.' ;;
  esac
  honeycomb_archive="honeycomb-$honeycomb_target.tar.gz"
  printf 'Downloading %s…\n' "$honeycomb_archive"
  curl --proto "$honeycomb_protocol" -fL --progress-bar --connect-timeout 20 --max-time 600 --retry 2 "$honeycomb_base/$honeycomb_archive" -o "$honeycomb_temp/$honeycomb_archive"
  curl --proto "$honeycomb_protocol" -fsSL --connect-timeout 20 --max-time 60 --retry 2 "$honeycomb_base/$honeycomb_archive.sha256" -o "$honeycomb_temp/checksum"
  honeycomb_expected="$(awk 'NR==1 {print $1}' "$honeycomb_temp/checksum")"
  [[ "$honeycomb_expected" =~ ^[a-fA-F0-9]{64}$ ]] || fail 'Release checksum has an invalid format.'
  if command -v sha256sum >/dev/null; then
    honeycomb_actual="$(sha256sum "$honeycomb_temp/$honeycomb_archive" | awk '{print $1}')"
  elif command -v shasum >/dev/null; then
    honeycomb_actual="$(shasum -a 256 "$honeycomb_temp/$honeycomb_archive" | awk '{print $1}')"
  else
    fail 'sha256sum or shasum is required for release verification.'
  fi
  [ "$honeycomb_actual" = "$honeycomb_expected" ] || fail 'SHA-256 verification failed. No executable was installed.'
  honeycomb_listing="$(tar -tzf "$honeycomb_temp/$honeycomb_archive")"
  [ "$honeycomb_listing" = honeycomb ] || fail 'Release archive must contain exactly one honeycomb executable.'
  tar -xzf "$honeycomb_temp/$honeycomb_archive" -C "$honeycomb_temp" -- honeycomb
  [ -f "$honeycomb_temp/honeycomb" ] && [ ! -L "$honeycomb_temp/honeycomb" ] || fail 'Release executable must be a regular file.'
  chmod 755 "$honeycomb_temp/honeycomb"
  "$honeycomb_temp/honeycomb" --version
  # Keep the replacement on the same filesystem so activation is atomic.
  cp "$honeycomb_temp/honeycomb" "$honeycomb_bin/.honeycomb.new.$$"
  chmod 755 "$honeycomb_bin/.honeycomb.new.$$"
  mv -f "$honeycomb_bin/.honeycomb.new.$$" "$honeycomb_bin/honeycomb"
fi

"$honeycomb_bin/honeycomb" --version
"$honeycomb_bin/honeycomb" config env > "$honeycomb_dir/env"
# The installer is a child process: startup files configure subsequent shells.
# Quote paths as shell data, including spaces, apostrophes and dollar signs.
printf -v honeycomb_env_quoted '%q' "$honeycomb_dir/env"
honeycomb_shell_line="[ ! -f $honeycomb_env_quoted ] || . $honeycomb_env_quoted"
honeycomb_shell_files=()
if [ "${HONEYCOMB_NO_MODIFY_PATH:-0}" != 1 ]; then
  case "${SHELL##*/}" in
    zsh) honeycomb_shell_files=("${ZDOTDIR:-$HOME}/.zshrc") ;;
    bash)
      honeycomb_shell_files=("$HOME/.bashrc")
      if [ -f "$HOME/.bash_profile" ]; then
        honeycomb_shell_files+=("$HOME/.bash_profile")
      elif [ -f "$HOME/.bash_login" ]; then
        honeycomb_shell_files+=("$HOME/.bash_login")
      else
        honeycomb_shell_files+=("$HOME/.profile")
      fi
      ;;
    sh|dash|ksh) honeycomb_shell_files=("$HOME/.profile") ;;
  esac
fi
for honeycomb_shell_file in "${honeycomb_shell_files[@]}"; do
  mkdir -p "$(dirname "$honeycomb_shell_file")"
  if ! grep -Fqx -- "$honeycomb_shell_line" "$honeycomb_shell_file" 2>/dev/null; then
    printf '\n# Honeycomb CLI\n%s\n' "$honeycomb_shell_line" >> "$honeycomb_shell_file"
  fi
  printf 'Shell configured: %s\n' "$honeycomb_shell_file"
done
if [ "${HONEYCOMB_NO_SERVICE:-0}" != 1 ]; then
  case "$(uname -s)" in
    Darwin)
      # launchd XML is generated with escaped values by the CLI, avoiding path interpolation.
      "$honeycomb_bin/honeycomb" service install
      ;;
    Linux)
      "$honeycomb_bin/honeycomb" service install
      ;;
  esac
fi
if [ "${#honeycomb_shell_files[@]}" -gt 0 ]; then
  printf '\nHoneycomb installed and added to your shell. Open a new terminal,\nor activate it in this terminal now:\n  . %s\n' "$honeycomb_env_quoted"
else
  printf '\nHoneycomb installed. Add it to your shell PATH with:\n  . %s\n' "$honeycomb_env_quoted"
fi
printf '\nThen run:\n  honeycomb --help\n  honeycomb login <slt>\n'
