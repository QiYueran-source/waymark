# Waymark

**Work items are Markdown files in your repo. No database, no daemon —
`git clone` is the whole install.**

English | [简体中文](README.zh-CN.md)

![Waymark banner](assets/banner-readme.png)

```sh
$ waymark init
initialised Waymark project at /home/you/demo

$ waymark create --title "Ship the first screen" --category task --owner "You"
created wm-f3c4djp65z9e

$ sed -n '1,5p' work/task/wm-f3c4djp65z9e.md
---
schema: 1
id: wm-f3c4djp65z9e
title: Ship the first screen
category: task
```

- **The documents are the source of truth.** One Markdown file per work item,
  YAML frontmatter, under `work/`; configuration in `.waymark/`. Git holds the
  history, so a ticket can be reviewed, diffed, and reverted like code.
- **Reads come from an in-memory projection — and say so.** Collection queries
  are answered from a projection rebuilt from the files and verified against
  their content fingerprints. A long-running view refreshes incrementally, so a
  document whose bytes did not change is not re-parsed. Every response records
  its `source`, `completeness`, and `freshness`, and a partial view fails with
  `INCOMPLETE_PROJECTION` instead of being published.
- **Agents work on the same project.** `waymark-mcp` is a Model Context Protocol
  server over stdio, so Claude Code, Cursor, or any MCP client reads and writes
  the work items you do.
- **Nothing to install at run time.** Prebuilt binaries for Linux, macOS, and
  Windows; no database, no daemon, and no network listener except the read-only
  Web view, which binds loopback only.

Pre-1.0: the document format and the CLI may still change between releases. The
macOS and Windows binaries are unsigned, so verify every archive against its
`.sha256` before running it.

**Not for you if** you need a board, notifications, or real-time multi-user
collaboration — those are deliberate omissions, not a roadmap gap.

## Packages

Both variants are built from the same tag and read the same project format:

- **Basic** — the `waymark` command-line tool and the `waymark-mcp` Model
  Context Protocol server.
- **Web** — the same commands plus the read-only Web view served by
  `waymark serve`.

A prebuilt Waymark needs no Rust toolchain, and the Web package needs no
Node.js or npm. Neither variant downloads or installs anything at run time.

## Install

Prebuilt archives are attached to the GitHub Releases for
`https://github.com/QiYueran-source/waymark`. Download the basic archive for your
platform, verify it, extract it, and put the extracted directory on `PATH`.

The archive names are:

| Platform | Archive |
| --- | --- |
| Linux x86_64 | `waymark-v<version>-linux-x86_64.tar.gz` |
| macOS, Apple silicon | `waymark-v<version>-macos-aarch64.tar.gz` |
| macOS, Intel | `waymark-v<version>-macos-x86_64.tar.gz` |
| Windows x86_64 | `waymark-v<version>-windows-x86_64.zip` |

Each Unix archive contains no top-level directory: it holds `waymark`,
`waymark-mcp`, `README.md`, `README.zh-CN.md`, `SECURITY.md`, `LICENSE-MIT`,
`LICENSE-APACHE`, and `THIRD_PARTY_LICENSES.txt` at its root. The Windows
archive holds the same files with the binaries named `waymark.exe` and
`waymark-mcp.exe`.
`THIRD_PARTY_LICENSES.txt` is the dependency-licence inventory for that build.

The examples below use version `0.1.0`. Set `VERSION` to the release you are
installing.

### Linux x86_64

```sh
VERSION=0.1.0
ARCHIVE="waymark-v${VERSION}-linux-x86_64.tar.gz"

curl -LO "https://github.com/QiYueran-source/waymark/releases/download/v${VERSION}/${ARCHIVE}"
curl -LO "https://github.com/QiYueran-source/waymark/releases/download/v${VERSION}/${ARCHIVE}.sha256"

sha256sum --check "${ARCHIVE}.sha256"

mkdir -p "$HOME/.local/waymark"
tar -xzf "${ARCHIVE}" -C "$HOME/.local/waymark"
export PATH="$HOME/.local/waymark:$PATH"
```

`sha256sum --check` must print `OK` for the archive and nothing else. A
checksum mismatch means the download is corrupt or not the published file; do
not run it.

### macOS, Apple silicon

```sh
VERSION=0.1.0
ARCHIVE="waymark-v${VERSION}-macos-aarch64.tar.gz"

curl -LO "https://github.com/QiYueran-source/waymark/releases/download/v${VERSION}/${ARCHIVE}"
curl -LO "https://github.com/QiYueran-source/waymark/releases/download/v${VERSION}/${ARCHIVE}.sha256"

shasum -a 256 -c "${ARCHIVE}.sha256"

mkdir -p "$HOME/.local/waymark"
tar -xzf "${ARCHIVE}" -C "$HOME/.local/waymark"
export PATH="$HOME/.local/waymark:$PATH"
```

### macOS, Intel

```sh
VERSION=0.1.0
ARCHIVE="waymark-v${VERSION}-macos-x86_64.tar.gz"

curl -LO "https://github.com/QiYueran-source/waymark/releases/download/v${VERSION}/${ARCHIVE}"
curl -LO "https://github.com/QiYueran-source/waymark/releases/download/v${VERSION}/${ARCHIVE}.sha256"

shasum -a 256 -c "${ARCHIVE}.sha256"

mkdir -p "$HOME/.local/waymark"
tar -xzf "${ARCHIVE}" -C "$HOME/.local/waymark"
export PATH="$HOME/.local/waymark:$PATH"
```

### Windows x86_64

The Windows archives are `.zip` files built with the MSVC toolchain. In
PowerShell:

```powershell
$Version = "0.1.0"
$Archive = "waymark-v$Version-windows-x86_64.zip"
$Base = "https://github.com/QiYueran-source/waymark/releases/download/v$Version"

Invoke-WebRequest "$Base/$Archive" -OutFile $Archive
Invoke-WebRequest "$Base/$Archive.sha256" -OutFile "$Archive.sha256"

$Expected = (Get-Content "$Archive.sha256") -split '\s+' | Select-Object -First 1
$Actual = (Get-FileHash $Archive -Algorithm SHA256).Hash.ToLower()
if ($Actual -ne $Expected) { throw "checksum mismatch for $Archive" }

Expand-Archive $Archive -DestinationPath "$HOME\waymark"
$env:Path = "$HOME\waymark;$env:Path"
```

`Get-FileHash` must match the first field of the `.sha256` file; a mismatch
means the download is corrupt or not the published file, so do not run it. The
`$env:Path` change lasts for the current session; add the directory through
**System Properties → Environment Variables** to keep it.

The Windows binaries are **not code-signed**. SmartScreen may show “Windows
protected your PC” on the first launch; after the checksum matched, choose
**More info → Run anyway**. Do not disable SmartScreen globally to run Waymark.

### macOS: Gatekeeper and an unsigned binary

The macOS archives are prebuilt but **not signed with an Apple Developer ID**
and **not notarized**. macOS therefore treats `waymark` and `waymark-mcp` as
software from an unidentified developer. This is expected, not a sign that the
download is damaged.

If you download and unpack with a browser, the first launch is blocked by
Gatekeeper with a message such as:

> “waymark” cannot be opened because the developer cannot be verified.

or, on newer macOS versions:

> Apple could not verify “waymark” is free of malware that may harm your Mac
> or compromise your privacy.

To allow the binary after checking that its SHA-256 matched:

1. Open **System Settings → Privacy & Security**.
2. Find the blocked `waymark` message and click **Open Anyway**.
3. Authenticate, then run the command again.

Gatekeeper checks each executable separately, so the first time an MCP client
launches `waymark-mcp` you may need to allow that binary the same way.

If the quarantine attribute is still present and you prefer the command line,
remove it from the verified binaries:

```sh
xattr -d com.apple.quarantine "$HOME/.local/waymark/waymark" \
  "$HOME/.local/waymark/waymark-mcp"
```

Only do this after the `.sha256` check passed and you trust the source. Do not
disable Gatekeeper globally to run Waymark.

### First commands

Check the installation, then create a project and its first work item:

```sh
waymark version
# waymark 0.1.0 (cli, mcp)

mkdir -p "$HOME/waymark-demo"
cd "$HOME/waymark-demo"

waymark init
# initialised Waymark project at /home/you/waymark-demo
#   configuration  /home/you/waymark-demo/.waymark/config.toml
#   local settings  /home/you/waymark-demo/.waymark/user.toml
#   category task  work/task

waymark create --title "Try Waymark" --category task --owner "Your Name"
# created wm-...
#   /home/you/waymark-demo/work/task/wm-....md

waymark list
# wm-...  open  ready  Try Waymark
```

`waymark init` creates `.waymark/config.toml` and the configured category
directories under `work/`. `waymark create` requires a title, a declared
category, and an owner. `waymark list` reads active work items.

`waymark --help` lists every command in this release, and
`waymark agent-doc` prints the same command reference as a Markdown capability
document for agents.

## The Web view

The Web view is a read-only browser view of a project. It is available only in
the **Web package**; it is not compiled into the basic package.

### What a basic build does

`waymark serve` exists in both packages, so the command is never a surprise:

```sh
waymark serve
# error: <message naming the Web package to install>
```

The basic build exits with the registered `WEB_NOT_INCLUDED` error (validation
exit class, process status 3) and names the Web package to install. It does not
download, install, or modify anything, and it does not start a server.

### Install the Web package

Download the Web archive for your platform from the same release:

- `waymark-web-v<version>-linux-x86_64.tar.gz`
- `waymark-web-v<version>-macos-aarch64.tar.gz`
- `waymark-web-v<version>-macos-x86_64.tar.gz`
- `waymark-web-v<version>-windows-x86_64.zip`

Verify its `.sha256` file the same way as the basic archive, then extract it
over the directory that already holds the basic installation:

```sh
VERSION=0.1.0
WEB_ARCHIVE="waymark-web-v${VERSION}-linux-x86_64.tar.gz"   # or the macOS archive
curl -LO "https://github.com/QiYueran-source/waymark/releases/download/v${VERSION}/${WEB_ARCHIVE}"
curl -LO "https://github.com/QiYueran-source/waymark/releases/download/v${VERSION}/${WEB_ARCHIVE}.sha256"
sha256sum --check "${WEB_ARCHIVE}.sha256"
tar -xzf "${WEB_ARCHIVE}" -C "$HOME/.local/waymark"
waymark version
# waymark 0.1.0 (cli, mcp, web)
```

On Windows, download `waymark-web-v<version>-windows-x86_64.zip` and its
`.sha256`, verify it with the same `Get-FileHash` comparison shown above, then
run `Expand-Archive $WEB_ARCHIVE -DestinationPath "$HOME\waymark" -Force` over
the basic installation.

The Web archive replaces the executables in the installation directory; no
project file is touched. The project's `.waymark/` directory and every
work-item document are unchanged, and no migration or re-initialisation is
needed: the basic and Web packages read and write the same project format.

### Run the server

From inside the project:

```sh
cd "$HOME/waymark-demo"
waymark serve
# serving http://127.0.0.1:7331
```

Open the printed URL. The service binds `127.0.0.1` only; there is no option
to expose it on the LAN or a public interface. The default port is `7331`.

Useful flags:

- `--port PORT` — bind another port. `--port 0` asks the operating system for
  a free port and prints the port that was chosen.
- `--open` — open the URL in the default browser after the server is bound.
  Waymark uses `xdg-open` on Linux and `open` on macOS; if the browser cannot
  be launched, that is a warning and the server keeps running. Without
  `--open`, no browser is opened.
- `--project DIR` — start project discovery at `DIR` instead of the current
  directory. The global flag works before or after the subcommand.

A busy port fails with the registered `PORT_IN_USE` error naming the port and
suggesting `--port 0`. Waymark never silently falls back to another port.

Readiness is printed on **stderr** as `serving http://127.0.0.1:<port>` before
the process blocks. With `--json`, the same URL is printed on stdout as the
`data` field of the versioned envelope, and the process still blocks serving
requests:

```sh
waymark serve --json
# {"v":1,"ok":true,"data":{"url":"http://127.0.0.1:7331"},"_meta":{...}}
```

Stop the server with `Ctrl-C`.

### Check the Web build's provenance

A Web build embeds a generated `assets-manifest.json`. Inspect it with:

```sh
waymark version --json | jq '.data.assets'
```

The manifest has this shape:

```json
{
  "version": "0.1.0",
  "git_sha": "<the tagged commit>",
  "build_id": "<the frontend build id>",
  "files": [{ "path": "index.html", "sha256": "<hash>", "size": 1234 }]
}
```

The manifest's `version` must equal the release version (`0.1.0`, the tag is
`v0.1.0`), `git_sha` must equal the commit that tag points at, and the
`index.html` entry's `sha256` must equal the SHA-256 of the `index.html` bytes
the server returns. With the server running:

```sh
curl -s http://127.0.0.1:7331/ | sha256sum
```

and compare that hash with the `index.html` entry. On macOS use
`shasum -a 256` in place of `sha256sum`.

A basic build has no `assets` field: there is no embedded frontend to report.
A local build made without `WAYMARK_GIT_SHA` has no `git_sha`; release builds
always do.

## Connect an agent over MCP

Both packages include `waymark-mcp`, a Model Context Protocol server over
stdio. Point an MCP client at the binary and, if the client does not start it
inside the project, at the project directory:

```sh
waymark-mcp --project /path/to/project
```

See [`docs/mcp-clients.md`](docs/mcp-clients.md) for working configuration
examples for Claude Code, DSH, and a generic JSON client.

## Build from source

The repository's Rust workspace is under `waymark/`. A basic build needs a Rust
toolchain (the workspace declares `rust-version = "1.85"`):

```sh
cd waymark
cargo build --release -p waymark-cli
cargo build --release -p waymark-mcp
```

The Web build embeds the frontend at compile time. Build the frontend first;
the Rust build does not run npm:

```sh
cd waymark/crates/waymark-web/ui
npm ci
npm run build
```

`npm run build` writes `crates/waymark-web/ui/build` and its hashed assets.
The Web build also needs the generated `assets-manifest.json` beside that tree;
the embedding build fails with instructions naming the missing file rather
than shipping an empty or stale frontend. The release workflow builds the
frontend once, generates the manifest, and reuses that tree for every Web
artifact. It sets `WAYMARK_SKIP_UI_BUILD=1` so later jobs reuse that tree; the
tree and manifest are still required, and the variable never substitutes a
placeholder.

Then build the CLI with the off-by-default `web` feature:

```sh
cd waymark
cargo build --release -p waymark-cli --features web
```

The resulting `waymark` is the Web variant; without `--features web` it is the
basic variant. There is no `full` alias. The release package for the source is
`waymark-src-v<version>.tar.gz` with a `.sha256` sibling, and it carries the
prebuilt frontend so it does not need to download assets.

## Verify a release

Every release archive has a `.sha256` sibling. The source tarball has one too.
To check downloaded artifacts, verify each checksum, confirm the archive
contents, run `waymark version`, and check a Web build's manifest and served
`index.html` hash as described above.

## Security

Do not open a public issue for a vulnerability. Report it through
[`SECURITY.md`](SECURITY.md), which also lists the supported versions and the
response targets.

## Licence

Waymark is dual-licensed under either the MIT Licence or the Apache License,
Version 2.0, at your option. See [`LICENSE-MIT`](LICENSE-MIT) and
[`LICENSE-APACHE`](LICENSE-APACHE). The third-party dependency licences for a
release are in `THIRD_PARTY_LICENSES.txt` in each archive.
