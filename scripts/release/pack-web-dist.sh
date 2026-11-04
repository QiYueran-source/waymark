#!/usr/bin/env bash
# Pack the one built frontend tree for a release.
#
# `web-dist` builds the frontend once and uploads the tree for every Web build
# leg. This script packs that exact tree (build/ + assets-manifest.json) as
# `waymark-web-dist-v<version>.tar.gz` with a `.sha256` sibling; the `source` job
# unpacks this archive over the tag's source tree, so the release's source
# tarball carries precisely the assets the binaries embed.
#
# Usage:
#   scripts/release/pack-web-dist.sh [--version VERSION] [--ui DIR]
#                                    [--git-sha SHA] [--out DIR]

set -euo pipefail
# shellcheck source=lib.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

version="$(workspace_version)"
ui="$UI_DIR"
git_sha=""
out_dir="$RELEASE_ROOT/dist"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --version) [[ $# -ge 2 ]] || fail "--version requires a value"; version="$2"; shift 2 ;;
        --ui) [[ $# -ge 2 ]] || fail "--ui requires a value"; ui="$2"; shift 2 ;;
        --git-sha) [[ $# -ge 2 ]] || fail "--git-sha requires a value"; git_sha="$2"; shift 2 ;;
        --out) [[ $# -ge 2 ]] || fail "--out requires a value"; out_dir="$2"; shift 2 ;;
        -h|--help)
            sed -n '2,14p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *) fail "unknown argument: $1" ;;
    esac
done

[[ -f "$ui/build/index.html" ]] || fail "the built frontend is missing at $ui/build"
[[ -f "$ui/assets-manifest.json" ]] || fail "the provenance manifest is missing at $ui/assets-manifest.json"

python3 - "$ui/assets-manifest.json" "$version" "$git_sha" <<'PY' || fail "the frontend manifest does not describe this release"
import json
import sys

path, version, git_sha = sys.argv[1], sys.argv[2], sys.argv[3]
with open(path, encoding="utf-8") as handle:
    manifest = json.load(handle)
problems = []
if manifest.get("version") != version:
    problems.append(f"manifest version {manifest.get('version')!r} != {version!r}")
if git_sha and manifest.get("git_sha") != git_sha:
    problems.append(f"manifest git_sha {manifest.get('git_sha')!r} != {git_sha!r}")
if not any(file.get("path") == "index.html" for file in manifest.get("files", [])):
    problems.append("manifest has no index.html entry")
if problems:
    print("; ".join(problems), file=sys.stderr)
    raise SystemExit(1)
PY

mkdir -p "$out_dir"
archive="$out_dir/waymark-web-dist-v$version.tar.gz"
tar -czf "$archive" -C "$ui" build assets-manifest.json
record_sha256 "$archive"
verify_sha256 "$archive"

printf 'packed %s\n' "$archive"
printf 'checksum %s\n' "$archive.sha256"
