#!/usr/bin/env bash
# Print the workspace version, or fail unless a release tag matches it.
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

set -euo pipefail
# shellcheck source=lib.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

tag=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --tag)
            [[ $# -ge 2 ]] || fail "--tag requires a value"
            tag="$2"
            shift 2
            ;;
        -h|--help)
            sed -n '2,12p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            fail "unknown argument: $1"
            ;;
    esac
done

version="$(workspace_version)"
[[ -n "$version" ]] || fail "could not read [workspace.package] version from $WAYMARK_DIR/Cargo.toml"

if [[ -n "$tag" ]]; then
    expected="v$version"
    [[ "$tag" == "$expected" ]] \
        || fail "release tag '$tag' does not match the workspace version; expected '$expected'"
fi

printf '%s\n' "$version"
