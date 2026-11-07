#!/usr/bin/env bash
# Build and package both Waymark variants for one release platform.
#
# The release workflow calls this once per matrix leg, and a maintainer calls it
# locally to produce the same Linux archives the workflow would. It does not run
# npm: the frontend tree and its manifest must already exist (build the frontend,
# generate the manifest, then package). `WAYMARK_SKIP_UI_BUILD=1` is exported to
# the Rust builds because the tree was built earlier in the same release, never
# to relax the requirement for index.html or assets-manifest.json.
#
# Both variants are built from one call so the basic and Web archives of a
# platform always share README.md, README.zh-CN.md, SECURITY.md, the licence
# files, and the licence inventory.
#
# Usage:
#   scripts/release/package.sh [--out DIR] [--version VERSION] [--git-sha SHA]
#                              [--target TRIPLE] [--os linux|macos|windows]
#                              [--arch x86_64|aarch64]
#                              [--licences FILE]
#
# The default platform is the host's; --target selects the Rust target triple for
# cross-compiling on a native runner. The archives are
#   waymark-v<version>-<os>-<arch>.tar.gz
#   waymark-web-v<version>-<os>-<arch>.tar.gz
# for linux and macos, and the matching .zip for windows, each with a .sha256
# sibling. A Windows archive holds waymark.exe and waymark-mcp.exe.

set -euo pipefail
# shellcheck source=lib.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"
PYTHON="$(python_cmd)"

version=""
git_sha=""
target=""
os=""
arch=""
out_dir="$RELEASE_ROOT/dist"
licences=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --out) [[ $# -ge 2 ]] || fail "--out requires a value"; out_dir="$2"; shift 2 ;;
        --version) [[ $# -ge 2 ]] || fail "--version requires a value"; version="$2"; shift 2 ;;
        --git-sha) [[ $# -ge 2 ]] || fail "--git-sha requires a value"; git_sha="$2"; shift 2 ;;
        --target) [[ $# -ge 2 ]] || fail "--target requires a value"; target="$2"; shift 2 ;;
        --os) [[ $# -ge 2 ]] || fail "--os requires a value"; os="$(normalise_os "$2")"; shift 2 ;;
        --arch) [[ $# -ge 2 ]] || fail "--arch requires a value"; arch="$(normalise_arch "$2")"; shift 2 ;;
        --licences) [[ $# -ge 2 ]] || fail "--licences requires a value"; licences="$2"; shift 2 ;;
        -h|--help)
            sed -n '2,25p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *) fail "unknown argument: $1" ;;
    esac
done

version="${version:-$(workspace_version)}"
git_sha="${git_sha:-$(git_sha_for HEAD)}"
[[ -n "$git_sha" ]] || fail "a release build needs WAYMARK_GIT_SHA / --git-sha"

if [[ -z "$os" || -z "$arch" ]]; then
    host="$(host_platform)"
    os="${os:-${host%%-*}}"
    arch="${arch:-${host##*-}}"
fi
platform="$os-$arch"
if [[ "$os" == windows && "$arch" != x86_64 ]]; then
    fail "the release publishes Windows x86_64 only; got $arch"
fi
archive_ext="$(archive_ext_for_os "$os")"

# The frontend is an explicit prior step. Never build it here and never accept a
# missing manifest: an archive without provenance is not a release artifact.
assets="$UI_DIR/build"
manifest="$UI_DIR/assets-manifest.json"
[[ -f "$assets/index.html" ]] \
    || fail "the built frontend is missing at $assets. Run: (cd $UI_DIR && npm ci && npm run build)"
[[ -f "$manifest" ]] \
    || fail "the provenance manifest is missing at $manifest. Run: cargo run -p waymark-web --bin waymark-web-manifest"
"$PYTHON" - "$manifest" "$version" "$git_sha" <<'PY' || fail "ASSETS_VERSION_MISMATCH: the frontend manifest does not describe the release being packaged"
import json
import sys

path, version, git_sha = sys.argv[1], sys.argv[2], sys.argv[3]
with open(path, encoding="utf-8") as handle:
    manifest = json.load(handle)
problems = []
if manifest.get("version") != version:
    problems.append(f"manifest version {manifest.get('version')!r} != {version!r}")
if manifest.get("git_sha") != git_sha:
    problems.append(f"manifest git_sha {manifest.get('git_sha')!r} != {git_sha!r}")
if problems:
    print("; ".join(problems), file=sys.stderr)
    raise SystemExit(1)
PY

export WAYMARK_SKIP_UI_BUILD=1
export WAYMARK_RELEASE=1
export WAYMARK_GIT_SHA="$git_sha"

cargo_build=(cargo build --release --locked --manifest-path "$WAYMARK_DIR/Cargo.toml")
if [[ -n "$target" ]]; then
    cargo_build+=(--target "$target")
fi

target_dir="$(cargo metadata --format-version 1 --no-deps --locked \
    --manifest-path "$WAYMARK_DIR/Cargo.toml" \
    | "$PYTHON" -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])')"
# cargo reports a native Windows path on the Windows runner, while every command
# in this script is a POSIX one, so convert the one path that comes back from a
# native tool.
if command -v cygpath >/dev/null 2>&1; then
    target_dir="$(cygpath -u "$target_dir")"
fi
bin_dir="$target_dir"
if [[ -n "$target" ]]; then
    bin_dir="$bin_dir/$target"
fi
bin_dir="$bin_dir/release"

work="$(mktemp -d "${TMPDIR:-/tmp}/waymark-package.XXXXXX")"
trap 'rm -rf "$work"' EXIT
stage_basic="$work/basic"
stage_web="$work/web"
mkdir -p "$stage_basic" "$stage_web" "$out_dir"

waymark_bin="$(binary_name waymark "$os")"
mcp_bin="$(binary_name waymark-mcp "$os")"

# The basic variant first: its waymark-mcp binary is the one both archives ship,
# and the Web build's waymark binary replaces the basic one in the same target
# directory.
printf '==> building the basic variant (%s)\n' "$platform"
"${cargo_build[@]}" -p waymark-cli -p waymark-mcp
cp "$bin_dir/$waymark_bin" "$bin_dir/$mcp_bin" "$stage_basic/"

printf '==> building the Web variant (%s)\n' "$platform"
"${cargo_build[@]}" -p waymark-cli --features web
cp "$bin_dir/$waymark_bin" "$stage_web/$waymark_bin"
cp "$stage_basic/$mcp_bin" "$stage_web/$mcp_bin"

licence_file="$work/THIRD_PARTY_LICENSES.txt"
if [[ -n "$licences" ]]; then
    [[ -f "$licences" ]] || fail "the supplied licence inventory is missing: $licences"
    cp "$licences" "$licence_file"
else
    "$RELEASE_ROOT/scripts/release/generate-licences.sh" --out "$licence_file"
fi
[[ -s "$licence_file" ]] || fail "the licence inventory is empty"

for stage in "$stage_basic" "$stage_web"; do
    cp "$RELEASE_ROOT/README.md" "$RELEASE_ROOT/README.zh-CN.md" \
        "$RELEASE_ROOT/SECURITY.md" "$RELEASE_ROOT/LICENSE-MIT" \
        "$RELEASE_ROOT/LICENSE-APACHE" "$stage/"
    cp "$licence_file" "$stage/THIRD_PARTY_LICENSES.txt"
    # Windows has no executable bit; the .exe extension carries the meaning.
    if [[ "$os" != windows ]]; then
        chmod 0755 "$stage/$waymark_bin" "$stage/$mcp_bin"
    fi
done

basic_archive="$out_dir/waymark-v$version-$platform.$archive_ext"
web_archive="$out_dir/waymark-web-v$version-$platform.$archive_ext"
archive_files=("$waymark_bin" "$mcp_bin" README.md README.zh-CN.md SECURITY.md LICENSE-MIT LICENSE-APACHE THIRD_PARTY_LICENSES.txt)

pack_archive() {
    local archive="$1" stage="$2"
    if [[ "$archive_ext" == "zip" ]]; then
        create_zip "$archive" "$stage" "${archive_files[@]}"
    else
        tar -czf "$archive" -C "$stage" "${archive_files[@]}" \
            || fail "tar failed for $archive"
    fi
    record_sha256 "$archive"
    verify_sha256 "$archive"
}

printf '==> packing %s\n' "$(basename "$basic_archive")"
pack_archive "$basic_archive" "$stage_basic"

printf '==> packing %s\n' "$(basename "$web_archive")"
pack_archive "$web_archive" "$stage_web"

printf '\npackaged %s:\n' "$platform"
printf '  %s\n  %s\n  %s\n  %s\n' \
    "$basic_archive" "$basic_archive.sha256" "$web_archive" "$web_archive.sha256"
