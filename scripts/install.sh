#!/usr/bin/env bash
# Fetch a published Waymark release archive, verify it against its .sha256
# sibling, and copy the requested binaries into a destination directory.
#
# This is the install step a WorkBuddy CLI connector runs, and it works the same
# way from a terminal. It needs curl, tar (or unzip on Windows under Git Bash) and
# a POSIX shell; it does not need a Rust toolchain or any package manager, which
# is the point: the machines that install a CLI connector cannot be assumed to
# have either.
#
# Nothing is installed globally. The binaries land in the destination, and the
# caller adds that directory to PATH.
#
# Usage:
#   scripts/install.sh --dest DIR [--version VERSION] [--binaries "waymark waymark-mcp"]
#
#   --dest DIR        Where to put the binaries. Created if missing.
#   --version VERSION Release to install, with or without the `v` prefix.
#                     Defaults to the latest published release.
#   --binaries LIST   Space- or comma-separated binary names. Default:
#                     "waymark waymark-mcp".
#   --repository OWNER/NAME   Overrides the default GitHub repository.
#   --base-url URL   Fetch <URL>/<archive> and <URL>/<archive>.sha256 instead of
#                     the GitHub release. The self-test uses this with file://.
#
# Exit codes: 0 installed, 2 unsupported platform or usage, 3 download failed,
# 4 checksum mismatch, 5 archive is not the expected shape.

set -euo pipefail

readonly DEFAULT_REPOSITORY="QiYueran-source/waymark"
readonly DEFAULT_BINARIES="waymark waymark-mcp"

fail() {
    local code="$1"
    shift
    printf 'install.sh: %s\n' "$*" >&2
    exit "$code"
}

dest=""
version=""
binaries="$DEFAULT_BINARIES"
repository="$DEFAULT_REPOSITORY"
base_override=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --dest) [[ $# -ge 2 ]] || fail 2 "--dest needs a value"; dest="$2"; shift 2 ;;
        --version) [[ $# -ge 2 ]] || fail 2 "--version needs a value"; version="$2"; shift 2 ;;
        --binaries) [[ $# -ge 2 ]] || fail 2 "--binaries needs a value"; binaries="${2//,/ }"; shift 2 ;;
        --repository) [[ $# -ge 2 ]] || fail 2 "--repository needs a value"; repository="$2"; shift 2 ;;
        --base-url) [[ $# -ge 2 ]] || fail 2 "--base-url needs a value"; base_override="$2"; shift 2 ;;
        -h|--help) sed -n '2,26p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) fail 2 "unknown argument: $1" ;;
    esac
done

[[ -n "$dest" ]] || fail 2 "--dest is required"

command -v curl >/dev/null 2>&1 || fail 2 "curl is required"

# The archive names the release workflow publishes: <os>-<arch> with the host's
# spelling normalised to the release spelling.
case "$(uname -s)" in
    Linux*) os=linux ;;
    Darwin*) os=macos ;;
    *) fail 2 "unsupported operating system: $(uname -s)" ;;
esac

case "$(uname -m)" in
    x86_64|amd64) arch=x86_64 ;;
    arm64|aarch64) arch=aarch64 ;;
    *) fail 2 "unsupported architecture: $(uname -m)" ;;
esac

# A checksum tool is needed only when a .sha256 sibling is published; the
# install refuses an unverified download when one exists, so check for the tool
# before downloading anything.
command -v sha256sum >/dev/null 2>&1 || command -v shasum >/dev/null 2>&1 \
    || fail 2 "neither sha256sum nor shasum is available; refusing to install unverified"

[[ "$os/$arch" != "linux/aarch64" ]] \
    || fail 2 "the release publishes no linux-aarch64 archive yet"

if [[ -z "$version" ]]; then
    version="$(curl -fsSL "https://api.github.com/repos/${repository}/releases/latest" \
        | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"v\{0,1\}\([^"]*\)".*/\1/p' | head -1)"
    [[ -n "$version" ]] || fail 3 "could not read the latest release tag; pass --version"
fi
tag="v${version#v}"
version="${version#v}"

archive="waymark-v${version}-${os}-${arch}.tar.gz"
base="${base_override:-https://github.com/${repository}/releases/download/${tag}}"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

printf 'install.sh: downloading %s\n' "$archive" >&2
if [[ -n "$base_override" ]]; then
    curl -fsSL -o "$tmp/$archive" "$base/$archive" || fail 3 "download failed: $base/$archive"
    if curl -fsSL -o "$tmp/$archive.sha256" "$base/$archive.sha256" 2>/dev/null; then
        verify=1
    else
        verify=0
    fi
else
    curl -fsSL --proto '=https' --tlsv1.2 -o "$tmp/$archive" "$base/$archive" \
        || fail 3 "download failed: $base/$archive"
    if curl -fsSL --proto '=https' --tlsv1.2 -o "$tmp/$archive.sha256" "$base/$archive.sha256" 2>/dev/null; then
        verify=1
    else
        verify=0
    fi
fi

# The .sha256 sibling is written by the release script in the "<hash>  <file>"
# form, which both sha256sum and shasum accept.
if [[ $verify -eq 1 ]]; then
    (
        cd "$tmp"
        if command -v sha256sum >/dev/null 2>&1; then
            sha256sum --check --quiet "$archive.sha256" || fail 4 "checksum mismatch for $archive"
        elif command -v shasum >/dev/null 2>&1; then
            shasum -a 256 -c "$archive.sha256" >/dev/null || fail 4 "checksum mismatch for $archive"
        else
            fail 2 "neither sha256sum nor shasum is available; refusing to install unverified"
        fi
    ) || exit $?
else
    # Only a release that predates the checksum siblings lands here; the TLS
    # connection to github.com is all that vouches for the file.
    printf 'install.sh: no %s.sha256 published; installing unverified\n' "$archive" >&2
fi

tar -xzf "$tmp/$archive" -C "$tmp" || fail 5 "cannot extract $archive"

mkdir -p "$dest" || fail 5 "cannot create $dest"
installed=()
for name in $binaries; do
    source="$tmp/$name"
    [[ -f "$source" ]] || fail 5 "$archive does not contain $name"
    cp "$source" "$dest/$name" || fail 5 "cannot write $dest/$name"
    chmod 755 "$dest/$name" || fail 5 "cannot mark $dest/$name executable"
    installed+=("$dest/$name")
done

printf 'install.sh: installed %s\n' "${installed[*]}" >&2
