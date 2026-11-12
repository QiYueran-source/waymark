#!/usr/bin/env bash
# Upload a downloaded release to Gitee: create the release when it is missing,
# attach only the files Gitee does not already have, then prove an anonymous
# download of one file matches what was uploaded.
#
# The files come from scripts/gitee-mirror-download.sh, which fetches them from
# GitHub and verifies their checksums. This half never talks to GitHub, so it
# runs as fast with no proxy as with one.
#
# Re-running is safe: a name that is already attached to the Gitee release is
# skipped, so an interrupted upload resumes instead of duplicating attachments.
#
# Usage:
#   scripts/gitee-release.sh --version 0.1.3 --src DIR [--repo OWNER/NAME]
#                            [--dry-run] [--only PATTERN]
#
#   --version VERSION  Release to publish, with or without the `v` prefix.
#   --src DIR          Directory holding the archives and their .sha256 files.
#   --repo OWNER/NAME  Gitee repository. Default: the `gitee` remote's repository.
#   --body TEXT        Release description for a release this creates.
#                      Default: a one-line pointer to the GitHub release.
#   --dry-run          Show what would be created and uploaded.
#   --only PATTERN     Upload only archives whose name matches the pattern.
#
# Required: the `gitee` CLI, logged in with a token that can manage releases.
# The token is read with `gitee auth token` and never written to disk.
#
# Exit codes: 0 done, 1 usage, 5 Gitee API failure.

set -euo pipefail
# shellcheck source=release/lib.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/release/lib.sh"

readonly GITEE_API="https://gitee.com/api/v5"
readonly GITEE_WEB="https://gitee.com"

version=""
src=""
gitee_repo=""
body=""
dry_run=0
only=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --version) [[ $# -ge 2 ]] || fail "--version needs a value"; version="$2"; shift 2 ;;
        --src) [[ $# -ge 2 ]] || fail "--src needs a value"; src="$2"; shift 2 ;;
        --repo) [[ $# -ge 2 ]] || fail "--repo needs a value"; gitee_repo="$2"; shift 2 ;;
        --body) [[ $# -ge 2 ]] || fail "--body needs a value"; body="$2"; shift 2 ;;
        --only) [[ $# -ge 2 ]] || fail "--only needs a value"; only="$2"; shift 2 ;;
        --dry-run) dry_run=1; shift ;;
        -h|--help) sed -n '2,27p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) fail "unknown argument: $1" ;;
    esac
done

[[ -n "$version" ]] || fail "--version is required"
[[ -n "$src" ]] || fail "--src is required"
[[ -d "$src" ]] || fail "--src is not a directory: $src"
tag="v${version#v}"

info() { printf 'gitee-release: %s\n' "$*" >&2; }

# The release object's leading "id" field. A greedy match over the whole body
# finds the author's id instead, so stop at the first field after "tag_name".
json_id() {
    printf '%s' "$1" | grep -o '^{"id":[0-9]\+' | head -1 | cut -d: -f2
}

command -v curl >/dev/null 2>&1 || fail "curl is required"
command -v gitee >/dev/null 2>&1 || fail "the gitee CLI is required; see https://gitee.com/oschina/gitee-cli"
token="$(gitee auth token 2>/dev/null | tr -d '\r\n')" || true
[[ -n "$token" ]] || fail "gitee is not logged in; run: gitee auth login"

if [[ -z "$gitee_repo" ]]; then
    url="$(git -C "$RELEASE_ROOT" remote get-url gitee 2>/dev/null || true)"
    [[ -n "$url" ]] || fail "no gitee remote; pass --repo OWNER/NAME"
    gitee_repo="$(printf '%s' "$url" | sed -e 's#^.*gitee\.com[:/]##' -e 's#\.git$##' -e 's#/$##')"
fi
[[ "$gitee_repo" == */* ]] || fail "cannot derive owner/repo from '$gitee_repo'"

# Every API call carries the token explicitly; nothing relies on the gitee CLI's
# own configuration for the request itself.
api() {
    local method="$1" path="$2"
    local separator='?'
    [[ "$path" == *'?'* ]] && separator='&'
    curl -sS --max-time 60 -X "$method" \
        "${GITEE_API}${path}${separator}access_token=${token}"
}

# The archives present in --src, one name per line, with their checksums.
archives="$(cd "$src" && ls -1 | grep -E '^(waymark|waymark-web|waymark-src)-v[0-9.]+-' \
    | grep -Ev '\.sha256$' | sort || true)"
[[ -n "$archives" ]] || fail "no release archives in $src"
if [[ -n "$only" ]]; then
    archives="$(printf '%s\n' "$archives" | grep -E "$only" || true)"
    [[ -n "$archives" ]] || fail "--only '$only' matched no archive"
fi

duplicates="$(printf '%s\n' "$archives" | sort | uniq -d)"
[[ -z "$duplicates" ]] || fail "the same archive appears twice: $duplicates"

info "publishing ${tag} to gitee.com/${gitee_repo} from ${src}"

# 1. Find or create the release. Gitee answers a missing tag with an HTTP error,
#    so the id is only read when the body is a release object.
release_id=""
release_json="$(api GET "/repos/${gitee_repo}/releases/tags/${tag}" || true)"
case "$release_json" in
    *'"tag_name"'*) release_id="$(json_id "$release_json")" ;;
esac
if [[ -n "$release_id" ]]; then
    info "release ${tag} exists (id ${release_id})"
elif [[ $dry_run -eq 1 ]]; then
    info "would create release ${tag}"
    release_id="0"
else
    info "creating release ${tag}"
    # Gitee requires a non-empty body and a target; omitting either answers 400.
    release_body="${body:-Waymark ${version}. Archives mirrored from the GitHub release; see the repository README for install steps.}"
    created="$(curl -sS --max-time 60 -H 'Content-Type: application/json' \
        --data "{\"tag_name\":\"${tag}\",\"name\":\"Waymark ${version}\",\"body\":\"${release_body}\",\"target_commitish\":\"master\",\"access_token\":\"${token}\"}" \
        "${GITEE_API}/repos/${gitee_repo}/releases")" \
        || fail 5 "could not create release ${tag}"
    release_id="$(json_id "$created")"
    if [[ -z "$release_id" ]]; then
        printf '%s\n' "$created" >&2
        fail 5 "release ${tag} was not created"
    fi
fi

# 2. Skip names Gitee already has. Names are unique per release, so this is the
#    whole idempotence rule.
existing=""
if [[ "$release_id" != "0" ]]; then
    existing="$(api GET "/repos/${gitee_repo}/releases/${release_id}/attach_files" \
        | grep -o '"name":"[^"]*"' | cut -d'"' -f4 | sort -u)"
fi

state="$(mktemp -d)"
trap 'rm -rf "$state"' EXIT
uploaded=0
skipped=0
for file in $archives; do
    for name in "$file" "$file.sha256"; do
        [[ -f "$src/$name" ]] || fail "missing $name in $src"
        if printf '%s\n' "$existing" | grep -qxF "$name"; then
            info "  present  $name"
            skipped=$((skipped + 1))
            continue
        fi
        if [[ $dry_run -eq 1 ]]; then
            info "  would upload $name"
            continue
        fi
        code="$(curl -sS --max-time 600 -o "$state/response.json" -w '%{http_code}' \
            -X POST -F "access_token=${token}" -F "file=@${src}/${name}" \
            "${GITEE_API}/repos/${gitee_repo}/releases/${release_id}/attach_files")"
        if [[ "$code" != "201" ]]; then
            cat "$state/response.json" >&2 || true
            fail 5 "upload failed for ${name} (HTTP ${code})"
        fi
        info "  uploaded $name"
        uploaded=$((uploaded + 1))
    done
done

if [[ $dry_run -eq 1 ]]; then
    info "dry run: nothing was created or uploaded"
    exit 0
fi

info "uploaded ${uploaded}, already present ${skipped}"

# 3. Prove an anonymous download matches. This is the property an install
#    command depends on, so it is checked rather than assumed.
probe="$(printf '%s\n' "$archives" | head -1)"
url="${GITEE_WEB}/${gitee_repo}/releases/download/${tag}/${probe}"
curl -fsSL --max-time 300 -o "$state/probe" "$url" || fail 5 "anonymous download failed: $url"
if command -v sha256sum >/dev/null 2>&1; then
    expected="$(sha256sum "$src/$probe" | cut -d' ' -f1)"
    actual="$(sha256sum "$state/probe" | cut -d' ' -f1)"
else
    expected="$(shasum -a 256 "$src/$probe" | cut -d' ' -f1)"
    actual="$(shasum -a 256 "$state/probe" | cut -d' ' -f1)"
fi
[[ "$expected" == "$actual" ]] || fail 5 "anonymous download of ${probe} does not match the uploaded file"
info "verified anonymous download: ${url}"
info "done: ${GITEE_WEB}/${gitee_repo}/releases/tag/${tag}"
