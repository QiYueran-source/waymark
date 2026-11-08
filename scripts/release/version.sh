#!/usr/bin/env bash
# Print the workspace version, fail unless a release tag matches it, or check
# that every published path dependency carries the same version.
#
# The release workflow runs this before it builds anything: a tag that does not
# equal [workspace.package] version must stop the pipeline rather than publish an
# artifact whose version lies. A maintainer runs the same check locally before
# tagging.
#
# Usage:
#   scripts/release/version.sh
#       Print the workspace version.
#   scripts/release/version.sh --tag v0.1.0
#       Fail unless v0.1.0 equals the workspace version; print the version.
#   scripts/release/version.sh --check
#       Fail unless every `[workspace.dependencies]` entry with a `path` under
#       `crates/` declares the workspace version. crates.io drops `path` when it
#       packages a crate, so a stale or missing `version` publishes a dependency
#       that does not exist.

set -euo pipefail
# shellcheck source=lib.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

tag=""
check=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --tag)
            [[ $# -ge 2 ]] || fail "--tag requires a value"
            tag="$2"
            shift 2
            ;;
        --check)
            check="1"
            shift
            ;;
        -h|--help)
            sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            fail "unknown argument: $1"
            ;;
    esac
done

version="$(workspace_version)"
[[ -n "$version" ]] || fail "could not read [workspace.package] version from $WAYMARK_DIR/Cargo.toml"

if [[ -n "$check" ]]; then
    status=0
    while IFS=' ' read -r member declared; do
        [[ -n "$member" ]] || continue
        if [[ "$declared" == "<missing>" ]]; then
            printf '%s: %s declares a path under crates/ without a version; crates.io cannot package it\n' \
                "$(basename "$0")" "$member" >&2
            status=1
        elif [[ "$declared" != "$version" ]]; then
            printf '%s: %s declares version %s but the workspace version is %s\n' \
                "$(basename "$0")" "$member" "$declared" "$version" >&2
            status=1
        fi
    done < <(workspace_path_dependency_versions)
    (( status == 0 )) || exit 1
    printf 'dependency versions match the workspace version %s\n' "$version"
    exit 0
fi

if [[ -n "$tag" ]]; then
    expected="v$version"
    [[ "$tag" == "$expected" ]] \
        || fail "release tag '$tag' does not match the workspace version; expected '$expected'"
fi

printf '%s\n' "$version"
