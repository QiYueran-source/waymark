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

# Fetch one asset by the id GitHub gave it. The /releases/download/<tag>/<file>
# URL answers 404 while the release is a draft, and a tag cannot be fetched
# anonymously at all; the asset id plus the octet-stream header works for both
# drafts and published releases.
fetch_asset() {
    local name="$1" id
    # The release JSON lists "url" .../assets/<id>, then "id", then "name"; the
    # id is the only stable handle for a draft's asset.
    id="$(printf '%s' "$window" | tr -d '\n' \
        | grep -oE "/assets/[0-9]+\",[^}]*\"name\": *\"${name}\"" \
        | grep -oE '^/assets/[0-9]+' | tr -d '/' | sed 's/assets//' | head -1)"
    if [[ -z "$id" ]]; then
        info "  no asset named ${name} in ${tag}"
        return 1
    fi
    curl -fsSL --proto '=https' --tlsv1.2 --retry 3 --max-time 1800 \
        "${auth_args[@]+"${auth_args[@]}"}" \
        -H 'Accept: application/octet-stream' \
        -o "$dest/$name" \
        "https://api.github.com/repos/${GITHUB_REPO}/releases/assets/${id}"
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
# Resolve the tag to a release through the list endpoint. GitHub answers
# /releases/tags/<tag> with 404 for a draft, and the tag page only lists a
# published release's assets, so the list is the one call that covers both.
auth_args=()
if [[ -n "${GH_TOKEN:-}" ]]; then
    auth_args=(-H "Authorization: Bearer ${GH_TOKEN}")
fi
list="$(curl -fsSL --retry 3 --max-time 120 "${auth_args[@]+"${auth_args[@]}"}" \
    "https://api.github.com/repos/${GITHUB_REPO}/releases?per_page=100")" \
    || fail 3 "could not read the releases of ${GITHUB_REPO} on GitHub (a draft needs GH_TOKEN)"
# One release object per array element. Split at the next tag_name (or the end)
# so asset names are read only from the release being mirrored; grepping the
# whole body collects every version's names.
window="$(printf '%s' "$list" | tr -d '\n' \
    | sed -e "s/\"tag_name\"/\n&/g" \
    | grep -F "\"tag_name\": \"${tag}\"" || true)"
[[ -n "$window" ]] || fail 3 "no release tagged ${tag} in ${GITHUB_REPO}"
assets="$(printf '%s' "$window" \
    | grep -oE '"name": *"(waymark|waymark-web|waymark-src)-v[0-9.][^"]*"' \
    | cut -d'"' -f4 \
    | grep -E '\.(tar\.gz|zip)$' \
    | grep -v '\.sha256$' \
    | sort -u || true)"
selected="$assets"
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
    fetch_asset "$file" || fail 3 "download failed: $file"
    fetch_asset "$file.sha256" || fail 3 "download failed: $file.sha256"
    verify "$file" || fail 4 "checksum mismatch: $file"
    fetched=$((fetched + 1))
done

info "fetched ${fetched}, already present ${cached}"
info "done; upload with: scripts/gitee-release.sh --version ${version} --src ${dest}"
