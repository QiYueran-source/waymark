#!/usr/bin/env bash
# Publish the workspace crates to crates.io, in dependency order.
#
# The order is the dependency graph: `waymark-core` first, then the adapters that
# declare it as a dependency. crates.io resolves a dependency from the index, so
# publishing an adapter before its dependency is visible fails with "no matching
# package named ...". This script waits for each version to appear in the index
# before it publishes the next crate, and skips any crate whose version is
# already published, so a re-run after a partial failure resumes instead of
# failing on "crate version already uploaded".
#
# Publication is irreversible: crates.io yanks a version, it does not delete one.
# The release workflow calls this only from an approval-gated environment; run it
# locally only when you mean to publish.
#
# Usage:
#   scripts/release/publish-crates.sh --version 0.1.0
#       Publish every crate at the workspace version. Needs
#       CARGO_REGISTRY_TOKEN in the environment.
#   scripts/release/publish-crates.sh --version 0.1.0 --dry-run
#       Package and verify each crate without uploading. Later crates still
#       fail if their dependency is not published yet, which is the real
#       ordering constraint, not a bug in this script.
#   scripts/release/publish-crates.sh --version 0.1.0 --crates "waymark-core"
#       Publish a subset, in the order given.

set -euo pipefail
# shellcheck source=lib.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

readonly DEFAULT_CRATES="waymark-core waymark-web waymark-mcp waymark-cli"
readonly INDEX_WAIT_SECONDS=300

version=""
crates="$DEFAULT_CRATES"
dry_run=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --version)
            [[ $# -ge 2 ]] || fail "--version requires a value"
            version="$2"
            shift 2
            ;;
        --crates)
            [[ $# -ge 2 ]] || fail "--crates requires a value"
            crates="$2"
            shift 2
            ;;
        --dry-run)
            dry_run=1
            shift
            ;;
        -h|--help)
            sed -n '2,25p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            fail "unknown argument: $1"
            ;;
    esac
done

if [[ -z "$version" ]]; then
    version="$(workspace_version)"
fi
[[ -n "$version" ]] || fail "could not read [workspace.package] version from $WAYMARK_DIR/Cargo.toml"

manifest_version="$(workspace_version)"
[[ "$manifest_version" == "$version" ]] \
    || fail "requested version '$version' is not the workspace version '$manifest_version'"

if [[ $dry_run -eq 0 ]]; then
    [[ -n "${CARGO_REGISTRY_TOKEN:-}" ]] \
        || fail "CARGO_REGISTRY_TOKEN is not set; generate a token at https://crates.io/settings/tokens"
fi

# The sparse-index URL for a crate, using crates.io's path rules: one-letter
# names live under 1/, two-letter under 2/, three-letter under 3/<first letter>/,
# everything else under <first two>/<next two>/.
crate_index_url() {
    local name="$1" path
    case "${#name}" in
        1) path="1/$name" ;;
        2) path="2/$name" ;;
        3) path="3/${name:0:1}/$name" ;;
        *) path="${name:0:2}/${name:2:2}/$name" ;;
    esac
    printf 'https://index.crates.io/%s\n' "$path"
}

# Succeed when the crate's index entry already lists the version. A missing
# crate (404) and a missing version both answer with "not published yet".
index_has_version() {
    local name="$1" wanted="$2" body
    body="$(curl -fsSL --max-time 20 "$(crate_index_url "$name")" 2>/dev/null)" || return 1
    while IFS= read -r line; do
        case "$line" in
            *"\"vers\":\"$wanted\""*) return 0 ;;
        esac
    done <<<"$body"
    return 1
}

wait_for_version() {
    local name="$1" waited=0
    while (( waited < INDEX_WAIT_SECONDS )); do
        if index_has_version "$name" "$version"; then
            printf '%s %s is visible in the index after %ss\n' "$name" "$version" "$waited"
            return 0
        fi
        sleep 5
        waited=$((waited + 5))
    done
    fail "$name $version did not appear in the crates.io index within ${INDEX_WAIT_SECONDS}s"
}

published=()
skipped=()

for crate in $crates; do
    if index_has_version "$crate" "$version"; then
        printf '%s: %s is already published; skipping\n' "$crate" "$version"
        skipped+=("$crate")
        continue
    fi

    if [[ $dry_run -eq 1 ]]; then
        printf '%s: packaging and verifying %s (dry run)\n' "$crate" "$version"
        ( cd "$WAYMARK_DIR" && cargo publish -p "$crate" --dry-run --allow-dirty )
        continue
    fi

    printf '%s: publishing %s\n' "$crate" "$version"
    ( cd "$WAYMARK_DIR" && cargo publish -p "$crate" --locked )
    wait_for_version "$crate"
    published+=("$crate")
done

if [[ $dry_run -eq 1 ]]; then
    printf 'dry run complete; nothing was uploaded\n'
    exit 0
fi

printf 'published: %s\n' "${published[*]:-none}"
printf 'already published: %s\n' "${skipped[*]:-none}"
