# Shared helpers for the Waymark release scripts.
#
# Every script under scripts/release/ sources this file. The helpers resolve the
# repository root from their own location, so a maintainer can run a script from
# any working directory, and they keep the artifact naming, checksum format, and
# version reading in exactly one place.

# shellcheck shell=bash

# The repository root, independent of the caller's working directory.
release_root() {
    local script_dir
    script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    cd "$script_dir/../.." && pwd
}

RELEASE_ROOT="$(release_root)"
readonly RELEASE_ROOT
readonly WAYMARK_DIR="$RELEASE_ROOT/waymark"
readonly UI_DIR="$WAYMARK_DIR/crates/waymark-web/ui"

# Print an error with the calling script's name and exit 1.
fail() {
    printf '%s: %s\n' "$(basename "$0")" "$*" >&2
    exit 1
}

# Read [workspace.package] version from the workspace manifest.
workspace_version() {
    awk '
        $0 == "[workspace.package]" { in_section = 1; next }
        /^\[/ { in_section = 0 }
        in_section && $0 ~ /^[[:space:]]*version[[:space:]]*=/ {
            line = $0
            sub(/^[^=]*=[[:space:]]*"/, "", line)
            sub(/".*$/, "", line)
            print line
            exit
        }
    ' "$WAYMARK_DIR/Cargo.toml"
}

# Every workspace path dependency and the version its entry declares, one
# "<member> <version>" line each, with the version printed as `<missing>` when
# the entry carries none. crates.io strips `path` at publish time, so a path
# dependency without a matching version either cannot be packaged at all or
# pins the published crate to a version that does not exist in the index.
workspace_path_dependency_versions() {
    awk '
        $0 == "[workspace.dependencies]" { in_deps = 1; next }
        /^\[/ { in_deps = 0 }
        in_deps && /path[[:space:]]*=[[:space:]]*"crates\// {
            name = $0
            sub(/[[:space:]]*=.*/, "", name)
            version = "<missing>"
            if (match($0, /version[[:space:]]*=[[:space:]]*"[^"]*"/)) {
                # Split on the quotes: the value is between the first two.
                split(substr($0, RSTART, RLENGTH), parts, "\"")
                version = parts[2]
            }
            print name, version
        }
    ' "$WAYMARK_DIR/Cargo.toml"
}

# The commit at HEAD, or the commit a revision resolves to.
git_sha_for() {
    local revision="${1:-HEAD}"
    git -C "$RELEASE_ROOT" rev-parse --verify "$revision^{commit}" 2>/dev/null \
        || fail "cannot resolve a Git commit for '$revision'"
}

# The SHA-256 of one file as lowercase hex.
sha256_file() {
    local file="$1"
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$file" | awk '{print $1}'
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$file" | awk '{print $1}'
    else
        fail "neither sha256sum nor shasum is installed"
    fi
}

# The Python 3 interpreter to use. The Windows runner's Git Bash does not
# reliably expose `python3`, so a script that needs Python asks for the name to
# use instead of assuming the Unix one.
python_cmd() {
    if command -v python3 >/dev/null 2>&1; then
        printf 'python3\n'
    elif command -v python >/dev/null 2>&1 \
        && python -c 'import sys; raise SystemExit(0 if sys.version_info[0] == 3 else 1)' 2>/dev/null; then
        printf 'python\n'
    else
        fail "a Python 3 interpreter is required (neither python3 nor python is available)"
    fi
}

# Write <archive>.sha256 containing "<hash>  <basename>", the format
# `sha256sum --check` expects on Linux and `shasum -a 256 --check` on macOS.
record_sha256() {
    local file="$1" directory base hash
    directory="$(cd "$(dirname "$file")" && pwd)"
    base="$(basename "$file")"
    hash="$(sha256_file "$directory/$base")"
    printf '%s  %s\n' "$hash" "$base" > "$directory/$base.sha256"
}

# Verify a file against its .sha256 sidecar without relying on sha256sum -c
# (macOS ships shasum, whose -c spelling differs only in the binary name).
verify_sha256() {
    local file="$1" sidecar="$1.sha256" expected actual
    [[ -f "$sidecar" ]] || fail "missing checksum sidecar: $sidecar"
    expected="$(awk 'NR == 1 { print $1 }' "$sidecar")"
    [[ "$expected" =~ ^[0-9a-f]{64}$ ]] \
        || fail "malformed checksum in $sidecar (expected 64 lowercase hex digits)"
    actual="$(sha256_file "$file")"
    [[ "$actual" == "$expected" ]] \
        || fail "checksum mismatch for $(basename "$file"): expected $expected, got $actual"
}

# The release artifact platform for this host, e.g. linux-x86_64. Git Bash
# reports `MINGW64_NT-…` (and MSYS/Cygwin report their own uname strings), so the
# Windows runner is recognised by the environment it is, not by the shell name.
host_platform() {
    local os arch
    os="$(uname -s | tr '[:upper:]' '[:lower:]')"
    case "$os" in
        darwin) os="macos" ;;
        linux) os="linux" ;;
        mingw*|msys*|cygwin*|windows*) os="windows" ;;
        *) fail "unsupported host operating system: $os" ;;
    esac
    arch="$(uname -m)"
    case "$arch" in
        arm64|aarch64) arch="aarch64" ;;
        x86_64|amd64) arch="x86_64" ;;
        *) fail "unsupported host architecture: $arch" ;;
    esac
    printf '%s-%s\n' "$os" "$arch"
}

# Normalise an OS name supplied by a caller (CI passes linux/macos/windows
# explicitly).
normalise_os() {
    case "$1" in
        linux|Linux) printf 'linux\n' ;;
        darwin|macos|macOS|MacOS) printf 'macos\n' ;;
        windows|Windows|win|Win) printf 'windows\n' ;;
        *) fail "unsupported release OS: $1" ;;
    esac
}

# The archive extension a release platform uses. Windows ships .zip, because
# that is what Windows users and tools expect; Unix ships .tar.gz.
archive_ext_for_os() {
    case "$1" in
        windows) printf 'zip\n' ;;
        linux|macos) printf 'tar.gz\n' ;;
        *) fail "unsupported release OS: $1" ;;
    esac
}

# Normalise an architecture name supplied by a caller.
normalise_arch() {
    case "$1" in
        x86_64|amd64) printf 'x86_64\n' ;;
        aarch64|arm64) printf 'aarch64\n' ;;
        *) fail "unsupported release architecture: $1" ;;
    esac
}

# The platform key embedded in an archive name, e.g. linux-x86_64.
archive_platform() {
    local name base rest
    name="$(basename "$1")"
    base="${name%.tar.gz}"
    base="${base%.zip}"
    base="${base%.tar}"
    case "$base" in
        waymark-src-*) fail "the source tarball has no platform: $name" ;;
        waymark-web-v*) rest="${base#waymark-web-v}" ;;
        waymark-v*) rest="${base#waymark-v}" ;;
        *) fail "cannot determine the platform of archive '$name'" ;;
    esac
    printf '%s\n' "$rest" | awk -F- '{ print $(NF - 1) "-" $NF }'
}

# The version embedded in an archive name.
archive_version() {
    local name base rest
    name="$(basename "$1")"
    base="${name%.tar.gz}"
    base="${base%.zip}"
    case "$base" in
        waymark-src-v*) rest="${base#waymark-src-v}" ;;
        waymark-web-v*) rest="${base#waymark-web-v}" ;;
        waymark-v*) rest="${base#waymark-v}" ;;
        *) fail "cannot determine the version of archive '$name'" ;;
    esac
    printf '%s\n' "${rest%-*-*}"
}

# The name a binary has inside a release archive for this platform.
binary_name() {
    local stem="$1" os="$2"
    case "$os" in
        windows) printf '%s.exe\n' "$stem" ;;
        linux|macos) printf '%s\n' "$stem" ;;
        *) fail "unsupported release OS: $os" ;;
    esac
}

# Extract a release archive (either format) into an existing directory.
extract_archive() {
    local archive="$1" dest="$2"
    case "$archive" in
        *.zip) extract_zip "$archive" "$dest" ;;
        *.tar.gz) tar -xzf "$archive" -C "$dest" ;;
        *) fail "unsupported archive format: $archive" ;;
    esac
}

# Extract a .zip with whichever tool the host has. `unzip` is the same binary on
# every Unix; the Windows runner may not ship it, so PowerShell is the native
# fallback there, and python3 is the last resort on either.
extract_zip() {
    local archive="$1" dest="$2"
    mkdir -p "$dest"
    if command -v unzip >/dev/null 2>&1; then
        unzip -q "$archive" -d "$dest" || fail "unzip failed for $archive"
        return
    fi
    if command -v powershell.exe >/dev/null 2>&1; then
        local archive_win dest_win
        command -v cygpath >/dev/null 2>&1 || fail "cygpath is required for the PowerShell zip fallback"
        archive_win="$(cygpath -w "$archive")"
        dest_win="$(cygpath -w "$dest")"
        powershell.exe -NoProfile -NonInteractive -Command \
            "Expand-Archive -LiteralPath '$archive_win' -DestinationPath '$dest_win' -Force" \
            || fail "Expand-Archive failed for $archive"
        return
    fi
    if command -v python3 >/dev/null 2>&1; then
        python3 -m zipfile -e "$archive" "$dest" || fail "python3 zipfile failed for $archive"
        return
    fi
    if py="$(python_cmd 2>/dev/null)"; then
        "$py" -m zipfile -e "$archive" "$dest" || fail "python zipfile failed for $archive"
        return
    fi
    fail "no unzip, PowerShell, or python3 is available to read $archive"
}

# Create a .zip holding exactly the named files at the archive root.
create_zip() {
    local out="$1" dir="$2"
    shift 2
    [[ $# -gt 0 ]] || fail "create_zip needs at least one file"
    local out_abs dir_abs
    out_abs="$(cd "$(dirname "$out")" && pwd)/$(basename "$out")"
    dir_abs="$(cd "$dir" && pwd)"

    if command -v zip >/dev/null 2>&1; then
        ( cd "$dir_abs" && zip -q -X "$out_abs" "$@" ) || fail "zip failed for $out"
        return
    fi
    if command -v powershell.exe >/dev/null 2>&1; then
        command -v cygpath >/dev/null 2>&1 || fail "cygpath is required for the PowerShell zip fallback"
        local out_win dir_win joined="" file
        out_win="$(cygpath -w "$out_abs")"
        dir_win="$(cygpath -w "$dir_abs")"
        for file in "$@"; do
            joined+="'$dir_win\\$file',"
        done
        joined="${joined%,}"
        powershell.exe -NoProfile -NonInteractive -Command \
            "Compress-Archive -Path $joined -DestinationPath '$out_win' -Force" \
            || fail "Compress-Archive failed for $out"
        return
    fi
    if command -v python3 >/dev/null 2>&1; then
        ( cd "$dir_abs" && python3 -m zipfile -c "$out_abs" "$@" ) \
            || fail "python3 zipfile failed for $out"
        return
    fi
    if py="$(python_cmd 2>/dev/null)"; then
        ( cd "$dir_abs" && "$py" -m zipfile -c "$out_abs" "$@" ) \
            || fail "python zipfile failed for $out"
        return
    fi
    fail "no zip, PowerShell, or python3 is available to create $out"
}

