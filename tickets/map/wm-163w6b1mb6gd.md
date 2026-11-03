---
schema: 1
id: wm-163w6b1mb6gd
title: Freeze the error taxonomy and exit-code mapping
category: map
status: closed
owner: frank
creator: frank
created_at: 2026-11-03T23:52:21Z
updated_at: 2026-11-03T23:52:22Z
closed_at: 2026-11-03T23:52:21Z
parent: wm-y14hs95k8wct
dependencies:
  - wm-2gc9r3qmthc4
  - wm-b34p3ccasdjd
  - wm-r2ddh3jgka17
  - wm-vx6w5s4m9mqh
---
## Question

What is the complete enumerable Waymark error taxonomy, and how does each code map to a CLI exit class?

The map currently lists codes as "such as", which means the set is open and adapters cannot be written against it. Freeze:

- the full code list, including the codes newly introduced by the editing and migration decisions;
- which codes are retryable and which are terminal;
- the typed `details` payload for each code, and which `location` shapes apply;
- the mapping of codes to the seven exit classes;
- which conditions are diagnostics (findings) rather than operation errors;
- how a schema/contract test pins the set so adding a code is deliberate.
