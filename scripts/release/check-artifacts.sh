#!/usr/bin/env bash
# Check Waymark release archives end to end.
#
# This is the script CI runs on ubuntu-latest against the downloaded draft
# artifacts and the script a maintainer runs against a local `dist/`. It is
# deliberately strict about what it can check and explicit about what it cannot:
#
#   * every archive has a .sha256 sibling and the checksum matches;
#   * a binary archive contains exactly the packaged file set, and the README,
#     licences, and cargo-about inventory are non-empty;
#   * the host-runnable binaries report the expected version and capabilities,
#     the basic build refuses `serve` with WEB_NOT_INCLUDED, and the Web build's
#     embedded manifest and served index.html pass the provenance check;
#   * a source archive carries the build tree and a manifest whose index.html
#     entry matches the actual file.
#
# A foreign-platform archive is checked structurally but its binaries are not
# executed: running a macOS binary on Linux is not possible, and pretending the
# check happened would be worse than saying it was skipped. Each native build leg
# runs this script before uploading, so every platform's binaries are executed
# somewhere.
#
# Usage:
#   scripts/release/check-artifacts.sh [--dir DIR] [--version VERSION]
#       [--git-sha SHA] [--platform OS-ARCH] [--manifest FILE]
#       [--require-platforms] [--no-exec]
#
# --platform limits the check to one leg (used by each build job).
# --require-platforms demands both variants for linux-x86_64, macos-aarch64, and
# macos-x86_64 plus the source tarball (used by the aggregate verification job).

set -euo pipefail
# shellcheck source=lib.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

dir="$RELEASE_ROOT/dist"
version="$(workspace_version)"
git_sha=""
platform=""
manifest=""
require_platforms=0
run_mode="auto"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --dir) [[ $# -ge 2 ]] || fail "--dir requires a value"; dir="$2"; shift 2 ;;
        --version) [[ $# -ge 2 ]] || fail "--version requires a value"; version="$2"; shift 2 ;;
        --git-sha) [[ $# -ge 2 ]] || fail "--git-sha requires a value"; git_sha="$2"; shift 2 ;;
        --platform) [[ $# -ge 2 ]] || fail "--platform requires a value"; platform="$2"; shift 2 ;;
        --manifest) [[ $# -ge 2 ]] || fail "--manifest requires a value"; manifest="$2"; shift 2 ;;
        --require-platforms) require_platforms=1; shift ;;
        --no-exec) run_mode="never"; shift ;;
        -h|--help)
            sed -n '2,36p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *) fail "unknown argument: $1" ;;
    esac
done

[[ -d "$dir" ]] || fail "the artifact directory does not exist: $dir"
dir="$(cd "$dir" && pwd)"
git_sha="${git_sha:-$(git_sha_for HEAD)}"
[[ -n "$git_sha" ]] || fail "a release check needs --git-sha / WAYMARK_GIT_SHA"
[[ -z "$manifest" || -f "$manifest" ]] || fail "the manifest to compare is missing: $manifest"
host="$(host_platform)"

tmp="$(mktemp -d "${TMPDIR:-/tmp}/waymark-artifacts.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT

shopt -s nullglob
basic_archives=("$dir"/waymark-v"$version"-*.tar.gz)
web_archives=("$dir"/waymark-web-v"$version"-*.tar.gz)
shopt -u nullglob
# The source tarball name has no wildcard; look for it explicitly so an absent
# source archive is not mistaken for a literal missing file.
source_archives=()
if [[ -f "$dir/waymark-src-v$version.tar.gz" ]]; then
    source_archives=("$dir/waymark-src-v$version.tar.gz")
fi

if [[ ${#basic_archives[@]} -eq 0 && ${#web_archives[@]} -eq 0 ]]; then
    fail "no Waymark binary archives for version $version in $dir"
fi

if [[ "$require_platforms" == 1 ]]; then
    if [[ -n "$platform" ]]; then
        fail "--require-platforms and --platform are mutually exclusive"
    fi
    for key in linux-x86_64 macos-aarch64 macos-x86_64; do
        for prefix in waymark waymark-web; do
            expected="$dir/$prefix-v$version-$key.tar.gz"
            [[ -f "$expected" ]] || fail "missing release artifact: $(basename "$expected")"
            [[ -f "$expected.sha256" ]] || fail "missing checksum: $(basename "$expected").sha256"
        done
    done
    [[ -f "$dir/waymark-src-v$version.tar.gz" ]] \
        || fail "missing release artifact: waymark-src-v$version.tar.gz"
    [[ -f "$dir/waymark-src-v$version.tar.gz.sha256" ]] \
        || fail "missing checksum: waymark-src-v$version.tar.gz.sha256"
fi

checked=0
executed=0
skipped=0

# Verify one extracted binary archive at `root`.
check_binary_tree() {
    local root="$1" variant="$2" label="$3" expected actual

    expected=$'LICENSE-APACHE\nLICENSE-MIT\nREADME.md\nTHIRD_PARTY_LICENSES.txt\nwaymark\nwaymark-mcp'
    actual="$(cd "$root" && find . -type f -print | sed 's|^\./||' | LC_ALL=C sort)"
    if [[ "$actual" != "$expected" ]]; then
        printf 'unexpected contents in %s:\n%s\nexpected:\n%s\n' "$label" "$actual" "$expected" >&2
        fail "archive contents do not match the release package"
    fi

    for file in README.md LICENSE-MIT LICENSE-APACHE THIRD_PARTY_LICENSES.txt; do
        [[ -s "$root/$file" ]] || fail "$label: $file is empty or missing"
    done
    grep -q "generated by cargo-about" "$root/THIRD_PARTY_LICENSES.txt" \
        || fail "$label: THIRD_PARTY_LICENSES.txt is not a cargo-about inventory"
    [[ -x "$root/waymark" ]] || fail "$label: waymark is not executable"
    [[ -x "$root/waymark-mcp" ]] || fail "$label: waymark-mcp is not executable"

    local archive_platform="${label#*-v$version-}"
    archive_platform="${archive_platform%.tar.gz}"
    if [[ "$run_mode" == "never" ]]; then
        printf 'skip: %s (--no-exec); binaries not run\n' "$label"
        skipped=$((skipped + 1))
        return
    fi
    if [[ "$archive_platform" != "$host" ]]; then
        printf 'skip: %s (built for %s, host is %s); checksum and contents checked, binaries not run\n' \
            "$label" "$archive_platform" "$host"
        skipped=$((skipped + 1))
        return
    fi

    local expected_line waymark_line mcp_line
    case "$variant" in
        basic) expected_line="waymark $version (cli, mcp)" ;;
        web) expected_line="waymark $version (cli, mcp, web)" ;;
        *) fail "unknown variant: $variant" ;;
    esac

    waymark_line="$("$root/waymark" version)" \
        || fail "$label: waymark version failed"
    [[ "$waymark_line" == "$expected_line" ]] \
        || fail "$label: waymark version printed '$waymark_line', expected '$expected_line'"

    mcp_line="$("$root/waymark-mcp" --version)" \
        || fail "$label: waymark-mcp --version failed"
    [[ "$mcp_line" == "waymark-mcp $version" ]] \
        || fail "$label: waymark-mcp printed '$mcp_line', expected 'waymark-mcp $version'"

    "$root/waymark" --json version > "$tmp/version.json" \
        || fail "$label: waymark --json version failed"
    python3 - "$tmp/version.json" "$version" "$variant" "$git_sha" <<'PY' || exit 1
import json
import sys

path, version, variant, git_sha = sys.argv[1:5]
with open(path, encoding="utf-8") as handle:
    data = json.load(handle).get("data", {})


def bad(message):
    print(f"check-artifacts: {message}", file=sys.stderr)
    raise SystemExit(1)


if data.get("version") != version:
    bad(f"the binary reports version {data.get('version')!r}, expected {version!r}")

expected = ["cli", "mcp"] if variant == "basic" else ["cli", "mcp", "web"]
if data.get("capabilities") != expected:
    bad(f"the binary reports capabilities {data.get('capabilities')!r}, expected {expected!r}")

if variant == "basic":
    if "assets" in data:
        bad("the basic build must not embed assets")
else:
    assets = data.get("assets")
    if not isinstance(assets, dict):
        bad("the Web build did not embed an assets manifest")
    if assets.get("version") != version:
        bad(f"the embedded manifest is for version {assets.get('version')!r}, expected {version!r}")
    if assets.get("git_sha") != git_sha:
        bad(f"the embedded manifest records commit {assets.get('git_sha')!r}, expected {git_sha!r}")
    files = assets.get("files")
    if not isinstance(files, list) or not any(file.get("path") == "index.html" for file in files):
        bad("the embedded manifest has no index.html entry")
PY

    if [[ "$variant" == "web" ]]; then
        local provenance_args=(--binary "$root/waymark" --version "$version" --git-sha "$git_sha")
        if [[ -n "$manifest" ]]; then
            provenance_args+=(--manifest "$manifest")
        fi
        "$RELEASE_ROOT/scripts/release/verify-web-provenance.sh" "${provenance_args[@]}" \
            || fail "$label: the Web provenance check failed"
    else
        # The basic build's serve must refuse rather than start a server. Its
        # failure is the registered WEB_NOT_INCLUDED, before any listener binds.
        local project="$tmp/basic-project-$checked"
        rm -rf "$project"
        "$root/waymark" init --project "$project" >"$tmp/basic-init.out" 2>"$tmp/basic-init.err" \
            || fail "$label: the packaged basic binary could not initialise a scratch project"
        local code=0
        set +e
        "$root/waymark" --json serve --project "$project" --port 0 \
            >"$tmp/basic-serve.json" 2>"$tmp/basic-serve.err"
        code=$?
        set -e
        [[ "$code" -ne 0 ]] || fail "$label: the basic build started serve instead of refusing"
        python3 - "$tmp/basic-serve.json" <<'PY' || exit 1
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    document = json.load(handle)
code = document.get("error", {}).get("code")
if code != "WEB_NOT_INCLUDED":
    print(f"check-artifacts: basic serve failed with {code!r}, expected 'WEB_NOT_INCLUDED'", file=sys.stderr)
    raise SystemExit(1)
PY
    fi

    executed=$((executed + 1))
    if [[ "$variant" == "web" ]]; then
        printf 'OK: %s (version, capabilities, provenance)\n' "$label"
    else
        printf 'OK: %s (version, capabilities, WEB_NOT_INCLUDED)\n' "$label"
    fi
}

# Verify one source archive at `root`.
check_source_tree() {
    local root="$1" label="$2" prefix="waymark-v$version"
    local tree="$root/$prefix"
    [[ -d "$tree" ]] || fail "$label: expected a $prefix/ directory at the archive root"

    for file in README.md LICENSE-MIT LICENSE-APACHE waymark/Cargo.toml \
        crates/waymark-web/ui/build/index.html \
        crates/waymark-web/ui/assets-manifest.json; do
        [[ -s "$tree/$file" ]] || fail "$label: missing $prefix/$file"
    done

    python3 - "$tree/crates/waymark-web/ui/assets-manifest.json" \
        "$tree/crates/waymark-web/ui/build/index.html" "$version" "$git_sha" <<'PY' || exit 1
import hashlib
import json
import sys
from pathlib import Path

manifest_path, index_path, version, git_sha = Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3], sys.argv[4]
with manifest_path.open(encoding="utf-8") as handle:
    manifest = json.load(handle)


def bad(message):
    print(f"ASSETS_VERSION_MISMATCH: {message}", file=sys.stderr)
    raise SystemExit(1)


if manifest.get("version") != version:
    bad(f"the source manifest is for version {manifest.get('version')!r}, expected {version!r}")
if manifest.get("git_sha") != git_sha:
    bad(f"the source manifest records commit {manifest.get('git_sha')!r}, expected {git_sha!r}")
entry = next((file for file in manifest.get("files", []) if file.get("path") == "index.html"), None)
if entry is None:
    bad("the source manifest has no index.html entry")
actual = hashlib.sha256(index_path.read_bytes()).hexdigest()
if actual != entry.get("sha256"):
    bad(f"{index_path} hashes to {actual}, which is not its manifest entry {entry.get('sha256')!r}")
PY

    printf 'OK: %s (source tree and manifest)\n' "$label"
}

for archive in "${basic_archives[@]}"; do
    archive_platform="$(archive_platform "$archive")"
    [[ -z "$platform" || "$archive_platform" == "$platform" ]] || continue
    [[ "$(archive_version "$archive")" == "$version" ]] \
        || fail "$(basename "$archive") is not version $version"
    verify_sha256 "$archive"
    label="$(basename "$archive")"
    root="$tmp/${label%.tar.gz}"
    mkdir -p "$root"
    tar -xzf "$archive" -C "$root"
    check_binary_tree "$root" basic "$label"
    checked=$((checked + 1))
done

for archive in "${web_archives[@]}"; do
    archive_platform="$(archive_platform "$archive")"
    [[ -z "$platform" || "$archive_platform" == "$platform" ]] || continue
    [[ "$(archive_version "$archive")" == "$version" ]] \
        || fail "$(basename "$archive") is not version $version"
    verify_sha256 "$archive"
    label="$(basename "$archive")"
    root="$tmp/${label%.tar.gz}"
    mkdir -p "$root"
    tar -xzf "$archive" -C "$root"
    check_binary_tree "$root" web "$label"
    checked=$((checked + 1))
done

# Bash 3.2 (the macOS runner default) rejects an empty array under `set -u`, so
# guard the loop instead of expanding an empty source_archives.
if [[ ${#source_archives[@]} -gt 0 ]]; then
    for archive in "${source_archives[@]}"; do
        [[ "$(archive_version "$archive")" == "$version" ]] \
            || fail "$(basename "$archive") is not version $version"
        verify_sha256 "$archive"
        label="$(basename "$archive")"
        root="$tmp/${label%.tar.gz}"
        mkdir -p "$root"
        tar -xzf "$archive" -C "$root"
        check_source_tree "$root" "$label"
        checked=$((checked + 1))
    done
fi

if [[ "$checked" -eq 0 ]]; then
    fail "no archives matched platform '${platform:-any}' and version $version in $dir"
fi

printf '\nchecked %s archive(s): %s executed, %s cross-platform skip(s)\n' \
    "$checked" "$executed" "$skipped"
