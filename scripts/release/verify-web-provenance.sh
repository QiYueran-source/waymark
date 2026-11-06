#!/usr/bin/env bash
# Verify that a packaged Web binary embeds the frontend manifest it claims and
# serves exactly the index.html that manifest lists.
#
# The check is deliberately end-to-end: it reads the provenance manifest out of
# the binary (`waymark --json version`), optionally compares it with the built
# tree's assets-manifest.json, starts the packaged `serve --port 0`, fetches `/`
# over real loopback HTTP, and compares the SHA-256 of the response with the
# manifest's index.html entry. Any disagreement is ASSETS_VERSION_MISMATCH, the
# same failure the specification names.
#
# Usage:
#   scripts/release/verify-web-provenance.sh --binary PATH --version VERSION \
#       --git-sha SHA [--manifest FILE] [--timeout SECONDS]
#
# The binary must be runnable on this host; callers skip a foreign-platform
# archive instead of attempting to execute it.

set -euo pipefail
# shellcheck source=lib.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"
PYTHON="$(python_cmd)"

binary=""
version=""
git_sha=""
manifest=""
timeout=30

while [[ $# -gt 0 ]]; do
    case "$1" in
        --binary) [[ $# -ge 2 ]] || fail "--binary requires a value"; binary="$2"; shift 2 ;;
        --version) [[ $# -ge 2 ]] || fail "--version requires a value"; version="$2"; shift 2 ;;
        --git-sha) [[ $# -ge 2 ]] || fail "--git-sha requires a value"; git_sha="$2"; shift 2 ;;
        --manifest) [[ $# -ge 2 ]] || fail "--manifest requires a value"; manifest="$2"; shift 2 ;;
        --timeout) [[ $# -ge 2 ]] || fail "--timeout requires a value"; timeout="$2"; shift 2 ;;
        -h|--help)
            sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *) fail "unknown argument: $1" ;;
    esac
done

[[ -n "$binary" ]] || fail "--binary is required"
# A Windows binary is waymark.exe: the extension decides execution, not a POSIX
# permission bit. Execution itself reports a missing or unusable binary.
[[ -f "$binary" ]] || fail "the Web binary is missing: $binary"
version="${version:-$(workspace_version)}"
git_sha="${git_sha:-$(git_sha_for HEAD)}"
[[ -n "$git_sha" ]] || fail "a release provenance check needs --git-sha / WAYMARK_GIT_SHA"
command -v curl >/dev/null 2>&1 || fail "curl is required to fetch the served index.html"
[[ "$timeout" =~ ^[0-9]+$ ]] || fail "--timeout must be a whole number of seconds"

tmp="$(mktemp -d "${TMPDIR:-/tmp}/waymark-provenance.XXXXXX")"
server_pid=""
cleanup() {
    if [[ -n "$server_pid" ]] && kill -0 "$server_pid" 2>/dev/null; then
        kill "$server_pid" 2>/dev/null || true
        wait "$server_pid" 2>/dev/null || true
    fi
    rm -rf "$tmp"
}
trap cleanup EXIT

# The binary's own account of itself, with the manifest a Web build embeds.
"$binary" --json version > "$tmp/version.json" \
    || fail "the packaged binary could not report its version"

# Read and check the version, commit, capability, and index.html entry. The
# Python exits 1 with an ASSETS_VERSION_MISMATCH line on any disagreement.
"$PYTHON" - "$tmp/version.json" "$version" "$git_sha" "$tmp/index.sha" <<'PY' || exit 1
import json
import sys

version_json, expected_version, expected_sha, index_sha_path = sys.argv[1:5]

with open(version_json, encoding="utf-8") as handle:
    data = json.load(handle).get("data", {})


def mismatch(message):
    print(f"ASSETS_VERSION_MISMATCH: {message}", file=sys.stderr)
    raise SystemExit(1)


if data.get("version") != expected_version:
    mismatch(f"the binary reports version {data.get('version')!r}, expected {expected_version!r}")

capabilities = data.get("capabilities")
if not isinstance(capabilities, list) or "web" not in capabilities:
    mismatch(f"the binary is not a Web build (capabilities {capabilities!r})")

assets = data.get("assets")
if not isinstance(assets, dict):
    mismatch("the Web build did not expose an embedded assets manifest")

if assets.get("version") != expected_version:
    mismatch(f"the embedded manifest is for version {assets.get('version')!r}, expected {expected_version!r}")
if assets.get("git_sha") != expected_sha:
    mismatch(f"the embedded manifest records commit {assets.get('git_sha')!r}, expected {expected_sha!r}")

files = assets.get("files")
if not isinstance(files, list):
    mismatch("the embedded manifest has no files list")
entry = next((file for file in files if file.get("path") == "index.html"), None)
if entry is None:
    mismatch("the embedded manifest has no index.html entry")
digest = entry.get("sha256")
if not isinstance(digest, str) or len(digest) != 64:
    mismatch(f"the embedded manifest's index.html hash is not sha256 hex: {digest!r}")

with open(index_sha_path, "w", encoding="utf-8") as handle:
    handle.write(digest)
PY

# When the built tree's manifest is available, prove the binary embeds exactly
# that tree and that the tree still hashes to the manifest's index entry.
if [[ -n "$manifest" ]]; then
    [[ -f "$manifest" ]] || fail "the manifest to compare is missing: $manifest"
    "$PYTHON" - "$manifest" "$tmp/version.json" "$version" "$git_sha" <<'PY' || exit 1
import hashlib
import json
import sys
from pathlib import Path

manifest_path, version_json, expected_version, expected_sha = sys.argv[1:5]
manifest_path = Path(manifest_path)

with manifest_path.open(encoding="utf-8") as handle:
    tree_manifest = json.load(handle)
with open(version_json, encoding="utf-8") as handle:
    embedded = json.load(handle).get("data", {}).get("assets", {})


def mismatch(message):
    print(f"ASSETS_VERSION_MISMATCH: {message}", file=sys.stderr)
    raise SystemExit(1)


if tree_manifest.get("version") != expected_version:
    mismatch(f"the built manifest is for version {tree_manifest.get('version')!r}, expected {expected_version!r}")
if tree_manifest.get("git_sha") != expected_sha:
    mismatch(f"the built manifest records commit {tree_manifest.get('git_sha')!r}, expected {expected_sha!r}")
if tree_manifest.get("files") != embedded.get("files"):
    mismatch("the built manifest and the embedded manifest disagree about the files or their hashes")

index_path = manifest_path.parent / "build" / "index.html"
if not index_path.is_file():
    print(f"verify-web-provenance: the built tree is missing {index_path}", file=sys.stderr)
    raise SystemExit(1)
actual = hashlib.sha256(index_path.read_bytes()).hexdigest()
entry = next((file for file in tree_manifest["files"] if file.get("path") == "index.html"), None)
if entry is None or entry.get("sha256") != actual:
    mismatch(f"{index_path} hashes to {actual}, which is not its manifest entry {entry and entry.get('sha256')!r}")
PY
fi

# Serve it for real. The scratch project is initialised by the packaged binary
# itself, so the check does not depend on the checkout's .waymark state.
project="$tmp/project"
if ! "$binary" init --project "$project" > "$tmp/init.out" 2> "$tmp/init.err"; then
    cat "$tmp/init.err" >&2
    fail "the packaged binary could not initialise a scratch project"
fi

"$binary" serve --project "$project" --port 0 > "$tmp/serve.out" 2> "$tmp/serve.err" &
server_pid=$!

ready=""
url=""
waited=0
while [[ "$waited" -lt $((timeout * 10)) ]]; do
    if ! kill -0 "$server_pid" 2>/dev/null; then
        cat "$tmp/serve.err" >&2
        fail "the packaged serve exited before reporting readiness"
    fi
    ready="$(grep -E -m1 -o 'serving http://127\.0\.0\.1:[0-9]+' "$tmp/serve.err" 2>/dev/null || true)"
    if [[ -n "$ready" ]]; then
        url="${ready#serving }"
        break
    fi
    sleep 0.1
    waited=$((waited + 1))
done
if [[ -z "$url" ]]; then
    cat "$tmp/serve.err" >&2
    fail "the packaged serve did not report readiness within ${timeout}s"
fi

# Embedded assets are served with `Content-Encoding: gzip`; --compressed asks
# curl to negotiate and decode it, so the hash is of the index.html bytes the
# manifest describes rather than of the gzip envelope.
status=""
if ! status="$(curl --silent --show-error --compressed --max-time 10 \
    --output "$tmp/served.html" --write-out '%{http_code}' "$url/")"; then
    fail "could not fetch $url/ from the packaged server"
fi
[[ "$status" == "200" ]] || fail "GET $url/ returned HTTP $status, expected 200"

served_sha="$(sha256_file "$tmp/served.html")"
embedded_sha="$(cat "$tmp/index.sha")"
if [[ "$served_sha" != "$embedded_sha" ]]; then
    printf 'ASSETS_VERSION_MISMATCH: the served index.html hashes to %s, but the embedded manifest records %s\n' \
        "$served_sha" "$embedded_sha" >&2
    exit 1
fi

kill "$server_pid" 2>/dev/null || true
wait "$server_pid" 2>/dev/null || true
server_pid=""

printf 'provenance OK: version %s, commit %s, index.html %s\n' "$version" "$git_sha" "$served_sha"
