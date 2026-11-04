---
schema: 1
id: wm-1zenrz88n4b4
title: Graph has an accessible structured alternative
category: implement
status: open
owner: frank
creator: frank
created_at: 2026-11-04T01:04:29Z
updated_at: 2026-11-04T01:04:42Z
parent: wm-7k3m4p9x2cqd
dependencies:
  - wm-kjewvvn9vrkb
  - wm-v0xand6t07b2
---
## What to build

A graph that only exists as geometry excludes the readers who most need the overview, so the graph gains a toggleable structured alternative: the same visible work as text and lists, readable without relying on edges drawn in space.

The alternative presents every visible node with its category and state, each node's parent-child relations, its dependency relations when dependency visibility is on, and the unresolved items with their reasons. It does not silently diverge from the graph: the same filters, the same collapsed nodes, and the same dependency toggle drive both, and the alternative's items are selectable so the same detail can be opened from it.

## Acceptance criteria

- [ ] a control toggles between the graph and its structured alternative, and the choice persists per project
- [ ] the alternative lists every node visible in the graph with its category and work state
- [ ] parent-child relations are stated for each node, not implied by indentation alone
- [ ] dependency relations appear only when dependency visibility is on, and state their direction
- [ ] unresolved items and their reasons are present in the alternative
- [ ] category filtering and collapse state apply equally to the alternative
- [ ] items in the alternative can be reached and selected by keyboard to open the same read-only detail
- [ ] the alternative remains a coherent overview when read as plain text, without colour or shape cues
- [ ] frontend tests cover the toggle and its persistence, node and relation content, dependency toggle parity, unresolved reasons, filter and collapse parity, and keyboard selection
