#!/usr/bin/env bash
# Build the release source tarball: `git archive` of the tag plus the built
# frontend tree.
#
# The source artifact is not a crate published to a registry. It is the tagged
# source with `crates/waymark-web/ui/build/` and `assets-manifest.json` added, so
# a user can rebuild the exact Web binary without downloading a second artifact
# or running npm. The tree comes from the release's `waymark-web-dist` archive
# (and its checksum is verified first), keeping the binaries, the source tarball,
# and the manifest in agreement.
#
# Usage:
#   scripts/release/make-source-tarball.sh --web-dist ARCHIVE [--rev REV]
#       [--version VERSION] [--git-sha SHA] [--out DIR]
#
# --rev defaults to HEAD; the workflow passes the release tag.

set -euo pipefail
# shellcheck source=lib.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

version="$(workspace_version)"
rev="HEAD"
git_sha=""
web_dist=""
out_dir="$RELEASE_ROOT/dist"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --version) [[ $# -ge 2 ]] || fail "--version requires a value"; version="$2"; shift 2 ;;
        --rev) [[ $# -ge 2 ]] || fail "--rev requires a value"; rev="$2"; shift 2 ;;
        --git-sha) [[ $# -ge 2 ]] || fail "--git-sha requires a value"; git_sha="$2"; shift 2 ;;
        --web-dist) [[ $# -ge 2 ]] || fail "--web-dist requires a value"; web_dist="$2"; shift 2 ;;
        --out) [[ $# -ge 2 ]] || fail "--out requires a value"; out_dir="$2"; shift 2 ;;
        -h|--help)
            sed -n '2,19p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *) fail "unknown argument: $1" ;;
    esac
done

[[ -n "$web_dist" ]] || fail "--web-dist is required"
[[ -f "$web_dist" ]] || fail "the frontend archive is missing: $web_dist"

git_sha="${git_sha:-$(git_sha_for "$rev")}"
[[ -n "$git_sha" ]] || fail "the source tarball needs a commit; pass --git-sha"

# The frontend archive is an input to the source artifact, so verify it before
# using it: a corrupt or swapped tree must not become part of the source release.
verify_sha256 "$web_dist"
dist_version="$(basename "$web_dist")"
dist_version="${dist_version#waymark-web-dist-v}"
dist_version="${dist_version%.tar.gz}"
[[ "$dist_version" == "$version" ]] \
    || fail "ASSETS_VERSION_MISMATCH: the frontend archive is for version $dist_version, expected $version"

mkdir -p "$out_dir"
work="$(mktemp -d "${TMPDIR:-/tmp}/waymark-source.XXXXXX")"
trap 'rm -rf "$work"' EXIT
staging="$work/staging"
prefix="waymark-v$version"
mkdir -p "$staging"

printf '==> archiving %s at %s\n' "$rev" "$git_sha"
git -C "$RELEASE_ROOT" archive --format=tar --prefix="$prefix/" "$rev" \
    | tar -xf - -C "$staging"

ui="$staging/$prefix/waymark/crates/waymark-web/ui"
mkdir -p "$ui"
printf '==> adding the built frontend from %s\n' "$(basename "$web_dist")"
tar -xzf "$web_dist" -C "$ui"

python3 - "$ui/assets-manifest.json" "$ui/build/index.html" "$version" "$git_sha" <<'PY' || fail "ASSETS_VERSION_MISMATCH: the source tarball's frontend does not describe this release"
import hashlib
import json
import sys
from pathlib import Path

manifest_path, index_path, version, git_sha = Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3], sys.argv[4]
with manifest_path.open(encoding="utf-8") as handle:
    manifest = json.load(handle)

problems = []
if manifest.get("version") != version:
    problems.append(f"manifest version {manifest.get('version')!r} != {version!r}")
if manifest.get("git_sha") != git_sha:
    problems.append(f"manifest git_sha {manifest.get('git_sha')!r} != {git_sha!r}")
entry = next((file for file in manifest.get("files", []) if file.get("path") == "index.html"), None)
if entry is None:
    problems.append("manifest has no index.html entry")
else:
    actual = hashlib.sha256(index_path.read_bytes()).hexdigest()
    if actual != entry.get("sha256"):
        problems.append(f"index.html hashes to {actual}, not its manifest entry {entry.get('sha256')!r}")
if problems:
    print("; ".join(problems), file=sys.stderr)
    raise SystemExit(1)
PY

archive="$out_dir/waymark-src-v$version.tar.gz"
printf '==> packing %s\n' "$(basename "$archive")"
tar -czf "$archive" -C "$staging" "$prefix"
record_sha256 "$archive"
verify_sha256 "$archive"

printf '\npackaged source tarball:\n  %s\n  %s\n' "$archive" "$archive.sha256"
