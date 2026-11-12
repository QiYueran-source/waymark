---
schema: 1
id: wm-8rr624fc9mjr
title: CLI and MCP agree on every cross-item write
category: implement
status: closed
owner: frank
creator: Frank
created_at: 2026-11-12T06:27:28Z
updated_at: 2026-11-12T10:55:25Z
closed_at: 2026-11-12T10:55:25Z
parent: wm-62yfmh5j41t5
dependencies:
  - wm-0c5qz2jk17sx
  - wm-0txgrh7efs7z
  - wm-4y03e40qvzy1
  - wm-9wz141hh9p73
  - wm-ckwnd9wq9jje
---
## Parent

[Cross-item write consent, archive safety, and dangling references](wm-62yfmh5j41t5)

## What to build

Prove the two surfaces behave as one. For each of the four cross-item operations, with and without consent, the CLI and the MCP server produce byte-identical documents, apply the same refusals, and report the same written and untouched documents. The capability parity suite covers the consent option by name on both sides.

## Acceptance criteria

- [ ] Equivalence tests cover `link add`, `link remove`, `archive`, and `create`, each with and without consent, comparing every persisted byte with operation-clock fields normalized
- [ ] Both refusals come back with the same code, exit class, and typed details from both adapters
- [ ] The parity suite covers the consent option on both surfaces, so a tool that lacks it fails the suite
- [ ] No adapter holds a rule of its own: removing the flag on one side breaks tests rather than quietly changing behaviour

## Blocked by

- wm-0c5qz2jk17sx — link add records the link on the caller's side only
- wm-0txgrh7efs7z — Refuse a missing consent and a foreign partner distinctly
- wm-ckwnd9wq9jje — link remove always works on one side
- wm-4y03e40qvzy1 — create does not rewrite its link partner by surprise
- wm-9wz141hh9p73 — Archive rewrites nothing but its target
