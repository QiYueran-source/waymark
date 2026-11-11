#!/usr/bin/env bash
# Check the WorkBuddy Connector package in connector/ against the published
# spec, and run every command the Skill quotes against a throwaway project.
#
# It checks the rules that are written down in the WorkBuddy connector docs:
# required meta fields, `source` shape, name and description lengths, the example
# count, a single MCP server when the package uses mcp.json, the files the chosen
# approach requires, the Skill front matter, and no secrets in any file. It then
# executes the documented CLI commands in a temporary project so a stale or
# invented command fails here rather than in the user's repository.
#
# What it cannot check: whether the Waymark CLI is reachable by the `curl | sh`
# install route (that needs a hosted installer, which does not exist yet), and
# whether the package passes WorkBuddy review.
#
# Usage:
#   scripts/workbuddy-connector/check.sh [--skip-command-run]

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
pkg="$root/connector"
waymark_dir="$root/waymark"
skip_run=0

for arg in "$@"; do
    case "$arg" in
        --skip-command-run) skip_run=1 ;;
        -h|--help) sed -n '2,18p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "unknown argument: $arg" >&2; exit 2 ;;
    esac
done

python="$(command -v python3 || true)"
[[ -n "$python" ]] || { echo "python3 is required" >&2; exit 2; }

failures=0

pass() { printf '  ok    %s\n' "$1"; }
fail() { printf '  FAIL  %s\n' "$1"; failures=$((failures + 1)); }

echo "Connector package: $pkg"

if [[ ! -f "$pkg/connector-meta.json" ]]; then
    fail "connector-meta.json is missing"
    exit 1
fi

"$python" - "$pkg" <<'PY'
import json, os, re, sys

pkg = sys.argv[1]
failures = 0

def ok(msg): print(f"  ok    {msg}")
def bad(msg):
    global failures
    print(f"  FAIL  {msg}")
    failures += 1

meta = json.load(open(os.path.join(pkg, "connector-meta.json")))

required = ["name", "name_en", "description", "description_zh", "description_en",
            "source", "examples_zh", "examples_en"]
for field in required:
    value = meta.get(field)
    if value in (None, "", []):
        bad(f"connector-meta.json: {field} is required")
    else:
        ok(f"connector-meta.json: {field}")

source = meta.get("source", "")
if re.fullmatch(r"[a-z0-9-]+", source or ""):
    ok(f"source is kebab-case: {source}")
else:
    bad(f"source must be lowercase letters, digits and hyphens: {source!r}")

name = meta.get("name", "")
if 2 <= len(name) <= 20:
    ok(f"name length {len(name)} is within 2-20")
else:
    bad(f"name length {len(name)} is outside 2-20")

for field in ("description_zh", "description_en"):
    value = meta.get(field, "")
    if 20 <= len(value) <= 100:
        ok(f"{field} length {len(value)} is within 20-100")
    else:
        bad(f"{field} length {len(value)} is outside 20-100")

for field in ("examples_zh", "examples_en"):
    n = len(meta.get(field, []))
    if 2 <= n <= 5:
        ok(f"{field} has {n} examples")
    else:
        bad(f"{field} has {n} examples; the spec asks for 2-5")

approach = meta.get("type", "mcp")
needed = ["connector-meta.json"]
if approach == "cli":
    needed.append("cli.json")
else:
    needed.append("mcp.json")
if not (os.path.exists(os.path.join(pkg, "icon.svg")) or os.path.exists(os.path.join(pkg, "icon.png"))):
    bad("an icon.svg or icon.png is required")
else:
    ok("icon present")

for name_ in needed:
    if os.path.exists(os.path.join(pkg, name_)):
        ok(f"{name_} present")
    else:
        bad(f"{name_} is required for the {approach} approach")

if approach == "cli":
    cli = json.load(open(os.path.join(pkg, "cli.json")))
    for field in ("init", "status", "unAuth"):
        commands = cli.get(field)
        if isinstance(commands, dict) and all(k in commands for k in ("darwin", "linux", "win32")):
            ok(f"cli.json: {field} covers darwin, linux, win32")
        else:
            bad(f"cli.json: {field} must give a command for darwin, linux and win32")
elif approach == "mcp":
    mcp = json.load(open(os.path.join(pkg, "mcp.json")))
    servers = mcp.get("mcpServers", {})
    if len(servers) == 1:
        ok("mcp.json has exactly one server")
    else:
        bad(f"mcp.json must contain exactly one server, found {len(servers)}")

skills_dir = os.path.join(pkg, "skills")
skill_files = []
for dirpath, _, filenames in os.walk(skills_dir):
    if "SKILL.md" in filenames:
        skill_files.append(os.path.join(dirpath, "SKILL.md"))
if skill_files:
    for path in skill_files:
        text = open(path, encoding="utf-8").read()
        head = text.split("---")[1] if text.startswith("---") else ""
        if "name:" in head and "description:" in head:
            ok(f"{os.path.relpath(path, pkg)} has name and description front matter")
        else:
            bad(f"{os.path.relpath(path, pkg)} needs name and description front matter")
else:
    bad("skills/<name>/SKILL.md is required")

secret = re.compile(r"(ghp_[A-Za-z0-9]{20,}|sk-[A-Za-z0-9]{20,}|AKIA[0-9A-Z]{16}|-----BEGIN [A-Z ]*PRIVATE KEY-----)")
leaks = []
for dirpath, _, filenames in os.walk(pkg):
    for name_ in filenames:
        path = os.path.join(dirpath, name_)
        if path.endswith((".png", ".jpg", ".ico")):
            continue
        if secret.search(open(path, encoding="utf-8", errors="ignore").read()):
            leaks.append(os.path.relpath(path, pkg))
if leaks:
    bad(f"possible secrets in {', '.join(leaks)}")
else:
    ok("no secrets in any package file")

sys.exit(1 if failures else 0)
PY
status=$?
if [[ $status -ne 0 ]]; then
    failures=$((failures + 1))
fi

if [[ $skip_run -eq 0 ]]; then
    echo "Running the documented commands against a temporary project"
    CLI="${WAYMARK_CLI:-$waymark_dir/target/debug/waymark}"
    if [[ ! -x "$CLI" ]]; then
        cargo build --quiet --manifest-path "$waymark_dir/Cargo.toml" --bin waymark >/dev/null
    fi
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' EXIT
    run() {
        if out="$("$@" 2>&1)"; then
            pass "$*"
        else
            fail "$* -> $out"
        fi
    }
    "$CLI" init --project "$tmp" >/dev/null 2>&1 || fail "waymark init"
    "$CLI" create --title "Smoke test" --category task --owner frank --project "$tmp" >/dev/null 2>&1 \
        || fail "waymark create"
    "$CLI" create --title "Second item" --category task --owner frank --project "$tmp" >/dev/null 2>&1 \
        || fail "waymark create (second item)"
    ids=()
    while IFS= read -r file; do
        ids+=("$(basename "$file" .md)")
    done < <(ls "$tmp"/work/task/*.md)
    id="${ids[0]}"
    other="${ids[1]}"
    run "$CLI" project info --json --project "$tmp"
    run "$CLI" --json list --project "$tmp"
    run "$CLI" --json list --status open --category task --project "$tmp"
    run "$CLI" show "$id" --json --project "$tmp"
    run "$CLI" ready --json --project "$tmp"
    run "$CLI" blocked --json --project "$tmp"
    run "$CLI" tree --depth 1 --json --project "$tmp"
    run "$CLI" dep tree "$id" --json --project "$tmp"
    run "$CLI" search "Smoke" --json --project "$tmp"
    run "$CLI" stats --json --project "$tmp"
    run "$CLI" title set "$id" "Renamed" --project "$tmp"
    printf 'body from the checker\n' > "$tmp/notes.md"
    run "$CLI" body replace "$id" --body-file "$tmp/notes.md" --project "$tmp"
    run "$CLI" status set "$id" in_progress --project "$tmp"
    run "$CLI" start "$id" --project "$tmp"
    run "$CLI" close "$id" --project "$tmp"
    run "$CLI" reopen "$id" --project "$tmp"
    run "$CLI" owner transfer "$id" frank --project "$tmp"
    run "$CLI" move "$id" task --project "$tmp"
    run "$CLI" parent set "$id" "$other" --project "$tmp"
    run "$CLI" parent clear "$id" --project "$tmp"
    run "$CLI" link add "$id" "$other" --project "$tmp"
    run "$CLI" link remove "$id" "$other" --project "$tmp"
    run "$CLI" dependency add "$id" "$other" --project "$tmp"
    run "$CLI" blocked --json --project "$tmp"
    run "$CLI" dependency remove "$id" "$other" --project "$tmp"
    run "$CLI" archive "$id" --project "$tmp"
    run "$CLI" validate --json --project "$tmp"
fi

echo
if [[ $failures -eq 0 ]]; then
    echo "connector package checks passed"
    exit 0
fi
echo "connector package checks failed: $failures problem(s)"
exit 1
