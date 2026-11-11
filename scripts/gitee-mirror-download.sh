#!/usr/bin/env bash
# Download one GitHub release and verify every asset against its .sha256 sibling.
#
# This is the first half of mirroring a release to Gitee; scripts/gitee-release.sh
# uploads what this leaves in the output directory. Splitting them lets the slow
# part (github.com from mainland China) run under whatever proxy curl is already
# using, while the upload half runs without one.
#
# Nothing here touches Gitee, and nothing here manages a proxy: curl reads
# HTTPS_PROXY / ALL_PROXY from the environment on its own, so start the proxy in
# the shell that runs this and stop worrying about it.
#
# The output directory holds only the release's archives and their .sha256
# siblings. Point it at a temporary directory, not at the repository.
#
# Usage:
#   scripts/gitee-mirror-download.sh --version 0.1.3 --dest DIR [--only PATTERN]
#
#   --version VERSION  Release to fetch, with or without the `v` prefix.
#   --dest DIR         Where to put the archives. Created if missing.
#   --only PATTERN     Fetch only assets whose name matches the pattern.
#
# Exit codes: 0 done, 1 usage, 3 download failed, 4 checksum mismatch.

set -euo pipefail
# shellcheck source=release/lib.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/release/lib.sh"

readonly GITHUB_REPO="QiYueran-source/waymark"

version=""
dest=""
only=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --version) [[ $# -ge 2 ]] || fail "--version needs a value"; version="$2"; shift 2 ;;
        --dest) [[ $# -ge 2 ]] || fail "--dest needs a value"; dest="$2"; shift 2 ;;
        --only) [[ $# -ge 2 ]] || fail "--only needs a value"; only="$2"; shift 2 ;;
        -h|--help) sed -n '2,26p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) fail "unknown argument: $1" ;;
    esac
done

[[ -n "$version" ]] || fail "--version is required"
[[ -n "$dest" ]] || fail "--dest is required"
tag="v${version#v}"

command -v curl >/dev/null 2>&1 || fail "curl is required"
command -v sha256sum >/dev/null 2>&1 || command -v shasum >/dev/null 2>&1 \
    || fail "neither sha256sum nor shasum is available"

info() { printf 'gitee-mirror-download: %s\n' "$*" >&2; }

sha_of() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d' ' -f1
    else
        shasum -a 256 "$1" | cut -d' ' -f1
    fi
}

verify() {
    local file="$1" expected actual
    [[ -f "$dest/$file.sha256" ]] || { info "missing $file.sha256"; return 1; }
    expected="$(cut -d' ' -f1 < "$dest/$file.sha256")"
    actual="$(sha_of "$dest/$file")"
    [[ "$expected" == "$actual" ]] || { info "checksum mismatch: $file"; return 1; }
}

# The archives, one name per line. The .sha256 siblings are fetched with each
# archive, so they are not part of the selection.
# The tag page carries the asset names. Filters instead of `sed … sort | head`
# on purpose: head exits early, the pipe closes, and curl dies of EPIPE with
# "unexpected eof while reading" (exit 56).
names="$(curl -fsSL --retry 3 --max-time 120 \
    "https://github.com/${GITHUB_REPO}/releases/expanded_assets/${tag}")" \
    || fail 3 "could not list the assets of ${tag} on GitHub"
assets="$(printf '%s' "$names" \
    | grep -o '\bwaymark[a-z-]*-v[0-9.]*-[a-z0-9_-]*\.\(tar\.gz\|zip\)' \
    | sort -u || true)"
[[ -n "$assets" ]] || fail "could not list the assets of ${tag} on GitHub"

selected="$(printf '%s\n' "$assets" | grep -E '^(waymark|waymark-web|waymark-src)-v[0-9.]+-' || true)"
[[ -n "$selected" ]] || fail "no archives in ${tag}"
if [[ -n "$only" ]]; then
    selected="$(printf '%s\n' "$selected" | grep -E "$only" || true)"
    [[ -n "$selected" ]] || fail "--only '$only' matched no archive"
fi

mkdir -p "$dest" || fail "cannot create $dest"

info "fetching ${GITHUB_REPO} ${tag} into ${dest}"
fetched=0
cached=0
for file in $selected; do
    if [[ -f "$dest/$file" ]] && verify "$file" 2>/dev/null; then
        info "  cached $file"
        cached=$((cached + 1))
        continue
    fi
    info "  fetch $file"
    curl -fsSL --proto '=https' --tlsv1.2 --retry 3 --max-time 1800 \
        -o "$dest/$file" \
        "https://github.com/${GITHUB_REPO}/releases/download/${tag}/${file}" \
        || fail 3 "download failed: $file"
    curl -fsSL --proto '=https' --tlsv1.2 --retry 3 --max-time 120 \
        -o "$dest/$file.sha256" \
        "https://github.com/${GITHUB_REPO}/releases/download/${tag}/${file}.sha256" \
        || fail 3 "download failed: $file.sha256"
    verify "$file" || fail 4 "checksum mismatch: $file"
    fetched=$((fetched + 1))
done

info "fetched ${fetched}, already present ${cached}"
info "done; upload with: scripts/gitee-release.sh --version ${version} --src ${dest}"
