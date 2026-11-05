---
schema: 1
id: wm-dq6f226w3g5k
title: Define safe temporary and recovery file diagnostics
category: map
status: open
owner: frank
created_at: 2026-11-05T02:09:09Z
updated_at: 2026-11-05T04:08:27Z
parent: wm-4wbvzsv34g2p
dependencies:
  - wm-a3ewqxebssk1
---
## Type

grilling

## Question

How do diagnostics identify and report possible temporary and recovery material after interruption without misclassifying active files, following untrusted links, modifying files, or treating them as authoritative? Define scope and behavior across supported platforms.

## Decisions confirmed so far

- Diagnostic checks for possible temporary or recovery material are bounded to known, safely accessible locations; they do not recursively scan the entire project or system to guarantee discovery. Report the checked scope and coverage that cannot be confirmed. A migration interruption may leave material in a former location no longer known from the current configuration, so no findings do not prove that no leftovers exist. Exact locations and inaccessible-scope behavior remain open.
