# WorkBuddy connector package

Files WorkBuddy needs to list Waymark in its Connector Marketplace, laid out as
the connector docs describe:

```text
connector/
├── connector-meta.json          marketplace metadata
├── cli.json                     install, status and logout commands
├── icon.svg                     package icon (icon.svg and icon.png are both accepted)
└── skills/
    └── waymark-work-items/
        └── SKILL.md             tells the assistant how to drive the CLI
```

Check it with `scripts/workbuddy-connector/check.sh`, which validates the
package against the published spec and runs every command the Skill quotes
against a throwaway project.

## Status: blocked on the install step, ready for everything else

The CLI approach is the only one that fits Waymark. The MCP approach in the same
docs requires a server that is reachable on the public network over HTTPS with a
recommended 99.9% SLA, and `waymark-mcp` is a stdio process that reads and writes
files in the user's own repository. It has no endpoint to reach.

The CLI approach has one hard requirement Waymark does not meet yet: **the CLI
has to be installable by a command that needs no Rust toolchain.** `cli.json`'s
`init` runs unattended on the user's machine, and WorkBuddy only prepares
Node.js and Python runtimes. `cargo install waymark-cli` fails on any machine
without `rustup`, which is most of them. One reported data point from
[ayukyo/icode-skill](https://github.com/ayukyo/icode-skill/blob/main/docs/workbuddy-support.md)
says their CLI carries a `curl | sh` installer; publishing one for Waymark
touches the install story this project has deliberately kept boring (no
installer script, checksum and archive per platform).

`cli.json` currently says `npm install -g waymark-mcp`, which does not exist.
The connector cannot be submitted until one of these is true:

1. A `curl | sh` installer exists and is hosted by the project, and `cli.json`
   points at it; or
2. An npm package that wraps the published binaries exists; or
3. WorkBuddy adds a runtime type that can install prebuilt binaries.

## What is ready

- `connector-meta.json` fields the docs mark required: name and English name,
  descriptions under 100 characters, three examples per language, kebab-case
  `source`, `type: "cli"`, and `version` matching the CLI.
- The `status` / `unAuth` commands in `cli.json`. Waymark has no login, so the
  docs' role for `status` becomes "is there a project above the working
  directory", and logging out is a no-op that returns 0. Both need two flags on
  `waymark-mcp` that **do not exist yet** (see below).
- `skills/waymark-work-items/SKILL.md`, with every quoted command executed by
  the checker. Four commands in the first draft were wrong (`body set`,
  `owner set`, `move --category`, `dependency add --blocking`) and the checker
  caught them.

## What is missing before submission

| Gap | Where | Cost |
| --- | --- | --- |
| `waymark-mcp --ready` and `--forget` | `crates/waymark-mcp/src/main.rs` | small: two flags on the binary |
| An install route with no Rust toolchain | release tooling | the real cost; needs a decision |
| `icon-64.png` and `icon-400.png` | `scripts/workbuddy-connector/build-icon.sh --render` | needs an SVG rasteriser; none is installed here, so only the SVG ships |

## Submitting

Submission is through the open platform, not through an API. The flow from the
[connector docs](https://open.workbuddy.cn/docs/connector):

1. Sign in at open.workbuddy.cn and pass identity verification. Individual
   developers need a mainland Chinese ID with face verification; there is no
   path for a personal developer outside mainland China. Enterprises need a
   business licence and either the legal representative's face check or a
   matching Tencent Cloud / WeChat account.
2. Add the asset in the connector section of the console, then trigger review.
3. After approval the connector sits in "pending release"; publishing it lists
   it in the marketplace.

The console requires a login, so nothing here can automate the submission. The
[Connector Meta tool](https://open.workbuddy.cn/tools/connector-meta) only
generates `connector-meta.json`; it does not upload the package. Questions go to
`openworkbuddy@tencent.com`.

## Which product this is for

WorkBuddy is Tencent's general-purpose office agent: documents, spreadsheets,
slides, data analysis. Its connector marketplace is built around that (QQ Mail,
Tencent Docs, TAPD, meetings). Waymark is a developer tracker, and its CLI only
makes sense inside a repository already holding a Waymark project. Listing there
is possible, but the audience overlap is small, which is why this package is
finished enough to submit and not worth finishing the install route for on its
own.
