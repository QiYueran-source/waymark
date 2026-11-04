# Shared helpers for the Waymark release scripts.
#
# Every script under scripts/release/ sources this file. The helpers resolve the
# repository root from their own location, so a maintainer can run a script from
# any working directory, and they keep the artifact naming, checksum format, and
# version reading in exactly one place.

# shellcheck shell=bash

# The repository root, independent of the caller's working directory.
release_root() {
    local script_dir
    script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    cd "$script_dir/../.." && pwd
}

RELEASE_ROOT="$(release_root)"
readonly RELEASE_ROOT
readonly WAYMARK_DIR="$RELEASE_ROOT/waymark"
readonly UI_DIR="$WAYMARK_DIR/crates/waymark-web/ui"

# Print an error with the calling script's name and exit 1.
fail() {
    printf '%s: %s\n' "$(basename "$0")" "$*" >&2
    exit 1
}

# Read [workspace.package] version from the workspace manifest.
workspace_version() {
    awk '
        $0 == "[workspace.package]" { in_section = 1; next }
        /^\[/ { in_section = 0 }
        in_section && $0 ~ /^[[:space:]]*version[[:space:]]*=/ {
            line = $0
            sub(/^[^=]*=[[:space:]]*"/, "", line)
            sub(/".*$/, "", line)
            print line
            exit
        }
    ' "$WAYMARK_DIR/Cargo.toml"
}

# The commit at HEAD, or the commit a revision resolves to.
git_sha_for() {
    local revision="${1:-HEAD}"
    git -C "$RELEASE_ROOT" rev-parse --verify "$revision^{commit}" 2>/dev/null \
        || fail "cannot resolve a Git commit for '$revision'"
}

# The SHA-256 of one file as lowercase hex.
sha256_file() {
    local file="$1"
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$file" | awk '{print $1}'
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$file" | awk '{print $1}'
    else
        fail "neither sha256sum nor shasum is installed"
    fi
}

# Write <archive>.sha256 containing "<hash>  <basename>", the format
# `sha256sum --check` expects on Linux and `shasum -a 256 --check` on macOS.
record_sha256() {
    local file="$1" directory base hash
    directory="$(cd "$(dirname "$file")" && pwd)"
    base="$(basename "$file")"
    hash="$(sha256_file "$directory/$base")"
    printf '%s  %s\n' "$hash" "$base" > "$directory/$base.sha256"
}

# Verify a file against its .sha256 sidecar without relying on sha256sum -c
# (macOS ships shasum, whose -c spelling differs only in the binary name).
verify_sha256() {
    local file="$1" sidecar="$1.sha256" expected actual
    [[ -f "$sidecar" ]] || fail "missing checksum sidecar: $sidecar"
    expected="$(awk 'NR == 1 { print $1 }' "$sidecar")"
    [[ "$expected" =~ ^[0-9a-f]{64}$ ]] \
        || fail "malformed checksum in $sidecar (expected 64 lowercase hex digits)"
    actual="$(sha256_file "$file")"
    [[ "$actual" == "$expected" ]] \
        || fail "checksum mismatch for $(basename "$file"): expected $expected, got $actual"
}

# The release artifact platform for this host, e.g. linux-x86_64.
host_platform() {
    local os arch
    os="$(uname -s | tr '[:upper:]' '[:lower:]')"
    case "$os" in
        darwin) os="macos" ;;
        linux) os="linux" ;;
        *) fail "unsupported host operating system: $os" ;;
    esac
    arch="$(uname -m)"
    case "$arch" in
        arm64|aarch64) arch="aarch64" ;;
        x86_64|amd64) arch="x86_64" ;;
        *) fail "unsupported host architecture: $arch" ;;
    esac
    printf '%s-%s\n' "$os" "$arch"
}

# Normalise an OS name supplied by a caller (CI passes linux/macos explicitly).
normalise_os() {
    case "$1" in
        linux|Linux) printf 'linux\n' ;;
        darwin|macos|macOS|MacOS) printf 'macos\n' ;;
        *) fail "unsupported release OS: $1" ;;
    esac
}

# Normalise an architecture name supplied by a caller.
normalise_arch() {
    case "$1" in
        x86_64|amd64) printf 'x86_64\n' ;;
        aarch64|arm64) printf 'aarch64\n' ;;
        *) fail "unsupported release architecture: $1" ;;
    esac
}

# The platform key embedded in an archive name, e.g. linux-x86_64.
archive_platform() {
    local name base rest
    name="$(basename "$1")"
    base="${name%.tar.gz}"
    base="${base%.tar}"
    case "$base" in
        waymark-src-*) fail "the source tarball has no platform: $name" ;;
        waymark-web-v*) rest="${base#waymark-web-v}" ;;
        waymark-v*) rest="${base#waymark-v}" ;;
        *) fail "cannot determine the platform of archive '$name'" ;;
    esac
    printf '%s\n' "$rest" | awk -F- '{ print $(NF - 1) "-" $NF }'
}

# The version embedded in an archive name.
archive_version() {
    local name base rest
    name="$(basename "$1")"
    base="${name%.tar.gz}"
    case "$base" in
        waymark-src-v*) rest="${base#waymark-src-v}" ;;
        waymark-web-v*) rest="${base#waymark-web-v}" ;;
        waymark-v*) rest="${base#waymark-v}" ;;
        *) fail "cannot determine the version of archive '$name'" ;;
    esac
    printf '%s\n' "${rest%-*-*}"
}

# The data object of a `waymark --json` envelope as one JSON document.
envelope_data() {
    local file="$1" out="$2"
    python3 - "$file" "$out" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    document = json.load(handle)
with open(sys.argv[2], "w", encoding="utf-8") as handle:
    json.dump(document.get("data", document), handle)
PY
}
