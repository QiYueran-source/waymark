---
schema: 1
id: wm-363qccgxcw0q
title: Give cross-item writes a place to say yes
category: implement
status: open
owner: frank
creator: Frank
created_at: 2026-11-12T06:27:26Z
updated_at: 2026-11-12T06:27:26Z
parent: wm-62yfmh5j41t5
---
## Parent

[Cross-item write consent, archive safety, and dangling references](wm-62yfmh5j41t5)

## What to build

Give the operations that can touch a second document a place to say "I meant this". The consent option and the shape of the reports that will explain which documents were written or left alone exist everywhere the adapters can reach them, but nothing changes behaviour yet: an operator running any command gets exactly what they get today.

This is the expand step: the new form exists beside the old, so every later ticket can land without breaking a call site.

## Acceptance criteria

- [ ] The operation options carry a consent flag that defaults to off, and every existing caller keeps its behaviour with no source change beyond the mechanical one
- [ ] The archive result can describe the documents it did not touch and the relation that referenced the target, even before anything populates it
- [ ] The CLI accepts the flag on `archive` and both `link` subcommands, and the MCP tools accept the equivalent boolean; both forward it to the domain layer, which still ignores it, and the existing CLI tests for `link` and `archive` keep passing with `--cascade` supplied
- [ ] The full workspace test suite passes unchanged
- [ ] The help text and the agent-facing capability reference list the new flag

## Blocked by

None (can start immediately).
