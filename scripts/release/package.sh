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
# platform always share README.md, the licence files, and the licence inventory.
#
# Usage:
#   scripts/release/package.sh [--out DIR] [--version VERSION] [--git-sha SHA]
#                              [--target TRIPLE] [--os linux|macos]
#                              [--arch x86_64|aarch64]
#                              [--licences FILE]
#
# The default platform is the host's; --target selects the Rust target triple for
# cross-compiling on a native runner. The archives are
#   waymark-v<version>-<os>-<arch>.tar.gz
#   waymark-web-v<version>-<os>-<arch>.tar.gz
# each with a .sha256 sibling.

set -euo pipefail
# shellcheck source=lib.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

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
            sed -n '2,26p' "$0" | sed 's/^# \{0,1\}//'
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

# The frontend is an explicit prior step. Never build it here and never accept a
# missing manifest: an archive without provenance is not a release artifact.
assets="$UI_DIR/build"
manifest="$UI_DIR/assets-manifest.json"
[[ -f "$assets/index.html" ]] \
    || fail "the built frontend is missing at $assets. Run: (cd $UI_DIR && npm ci && npm run build)"
[[ -f "$manifest" ]] \
    || fail "the provenance manifest is missing at $manifest. Run: cargo run -p waymark-web --bin waymark-web-manifest"
python3 - "$manifest" "$version" "$git_sha" <<'PY' || fail "the frontend manifest does not describe the release being packaged"
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
    | python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])')"
bin_dir="$target_dir"
[[ -n "$target" ]] && bin_dir="$bin_dir/$target"
bin_dir="$bin_dir/release"

work="$(mktemp -d "${TMPDIR:-/tmp}/waymark-package.XXXXXX")"
trap 'rm -rf "$work"' EXIT
stage_basic="$work/basic"
stage_web="$work/web"
mkdir -p "$stage_basic" "$stage_web" "$out_dir"

# The basic variant first: its waymark-mcp binary is the one both archives ship,
# and the Web build's waymark binary replaces the basic one in the same target
# directory.
printf '==> building the basic variant (%s)\n' "$platform"
"${cargo_build[@]}" -p waymark-cli -p waymark-mcp
cp "$bin_dir/waymark" "$bin_dir/waymark-mcp" "$stage_basic/"

printf '==> building the Web variant (%s)\n' "$platform"
"${cargo_build[@]}" -p waymark-cli --features web
cp "$bin_dir/waymark" "$stage_web/waymark"
cp "$stage_basic/waymark-mcp" "$stage_web/waymark-mcp"

licence_file="$work/THIRD_PARTY_LICENSES.txt"
if [[ -n "$licences" ]]; then
    [[ -f "$licences" ]] || fail "the supplied licence inventory is missing: $licences"
    cp "$licences" "$licence_file"
else
    "$RELEASE_ROOT/scripts/release/generate-licences.sh" --out "$licence_file"
fi
[[ -s "$licence_file" ]] || fail "the licence inventory is empty"

for stage in "$stage_basic" "$stage_web"; do
    cp "$RELEASE_ROOT/README.md" "$RELEASE_ROOT/LICENSE-MIT" "$RELEASE_ROOT/LICENSE-APACHE" "$stage/"
    cp "$licence_file" "$stage/THIRD_PARTY_LICENSES.txt"
    chmod 0755 "$stage/waymark" "$stage/waymark-mcp"
done

basic_archive="$out_dir/waymark-v$version-$platform.tar.gz"
web_archive="$out_dir/waymark-web-v$version-$platform.tar.gz"
archive_files=(waymark waymark-mcp README.md LICENSE-MIT LICENSE-APACHE THIRD_PARTY_LICENSES.txt)

printf '==> packing %s\n' "$(basename "$basic_archive")"
tar -czf "$basic_archive" -C "$stage_basic" "${archive_files[@]}"
record_sha256 "$basic_archive"
verify_sha256 "$basic_archive"

printf '==> packing %s\n' "$(basename "$web_archive")"
tar -czf "$web_archive" -C "$stage_web" "${archive_files[@]}"
record_sha256 "$web_archive"
verify_sha256 "$web_archive"

printf '\npackaged %s:\n' "$platform"
printf '  %s\n  %s\n  %s\n  %s\n' \
    "$basic_archive" "$basic_archive.sha256" "$web_archive" "$web_archive.sha256"
