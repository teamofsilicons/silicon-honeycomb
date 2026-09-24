## One archive, six native targets
A Honeycomb release is a gzip-compressed tar archive with `honeycomb.yaml` at its root and platform files below `targets/`. Do not wrap these in an extra parent directory.
```text
my-app/
  honeycomb.yaml
  targets/
    linux-x86_64/my-app
    linux-aarch64/my-app
    windows-x86_64/my-app.exe
    windows-aarch64/my-app.exe
    macos-x86_64/my-app
    macos-aarch64/my-app
```
## Complete manifest
```yaml
format_version: 1
# app_id: my-app  # Optional
version: 1.0.0
bin:
  my-app: main
targets:
  linux-x86_64:
    root: targets/linux-x86_64
    executables:
      main: my-app
  linux-aarch64:
    root: targets/linux-aarch64
    executables:
      main: my-app
  windows-x86_64:
    root: targets/windows-x86_64
    executables:
      main: my-app.exe
  windows-aarch64:
    root: targets/windows-aarch64
    executables:
      main: my-app.exe
  macos-x86_64:
    root: targets/macos-x86_64
    executables:
      main: my-app
  macos-aarch64:
    root: targets/macos-aarch64
    executables:
      main: my-app
```
[Download this manifest](/examples/honeycomb.yaml). Replace the command and payload names with your own.

## Field reference
| Field | Meaning |
| --- | --- |
| `format_version` | Must be `1`. |
| `app_id` | Optional permanent `app` identity. If present, it must match the application selected for upload or installation. If omitted, the selected application and its release record provide the identity. |
| `version` | Semantic release version, for example `1.0.0`. |
| `bin` | Public command name → logical executable identifier. At least one command is required. |
| `targets` | Target ID → root directory and executable mappings. |
| `targets.<id>.root` | Safe relative directory below `targets/`; target roots cannot overlap. |
| `targets.<id>.executables` | Logical executable identifier → path relative to that target root. |

For example, `bin: { my-app: main }` exposes `my-app` on PATH and resolves `main` separately for each platform. Multiple commands may map to logical executables. Unknown manifest fields are rejected.

## Supported targets
Required: `linux-x86_64`, `linux-aarch64`, `windows-x86_64`, `windows-aarch64`, `macos-x86_64`, `macos-aarch64`.

Optional: `linux-i686`, `linux-armv7hf`, `windows-i686`. Other target identifiers are rejected. These are Honeycomb target names, not Rust target triples.

## Limits and validation
| Limit | Value |
| --- | --- |
| Compressed archive | 512 MiB |
| Total expanded files | 2 GiB |
| Archive entries | 50,000 |
| Manifest size | 1 MiB |

Links (symbolic or hard), special files, traversal, absolute paths, duplicate/case-colliding paths, and unexpected top-level entries are rejected. Use portable file names; Windows reserved names and unsafe characters are forbidden. Non-Windows executables must have an executable permission bit when validated on Unix.

```sh
chmod +x my-app/targets/linux-*/my-app my-app/targets/macos-*/my-app
honeycomb validate my-app
honeycomb pack my-app --output my-app-1.0.0.tar.gz
```
Package runtime assets inside the appropriate target root. Do not include credentials, source working trees, or installer scripts that mutate the host. The installer selects a target, validates and stages its payload, then activates owned commands. It does not run package lifecycle scripts.

Without `--output`, the archive is named `<app-handle>-<version>.tar.gz` when `app_id` is set. Otherwise Honeycomb uses the alphabetically first command in `bin`, for example `my-app-1.0.0.tar.gz`. The archive bytes are preserved during upload.
