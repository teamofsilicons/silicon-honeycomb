#!/bin/bash
# Honeycomb installer: prebuilt release + SHA-256, or an explicit local source checkout.
set -euo pipefail
umask 077

fail() { printf 'honeycomb installer: %s\n' "$*" >&2; exit 1; }
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
  curl --proto "$honeycomb_protocol" -fsSL --retry 2 "$honeycomb_base/$honeycomb_archive" -o "$honeycomb_temp/$honeycomb_archive"
  curl --proto "$honeycomb_protocol" -fsSL --retry 2 "$honeycomb_base/$honeycomb_archive.sha256" -o "$honeycomb_temp/checksum"
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
printf '\nHoneycomb installed. Add it to your shell:\n  source "%s/env"\n\nThen run:\n  honeycomb --help\n  honeycomb login <slt>\n' "$honeycomb_dir"
