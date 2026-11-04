---
schema: 1
id: wm-wtb2rpjn12bm
title: Only blocking and empty states are centred
category: implement
status: open
owner: frank
created_at: 2026-11-04T12:57:13Z
updated_at: 2026-11-04T12:57:13Z
parent: wm-7k3m4p9x2cqd
---
## What to build

Centre only blocking and empty states: no category selected, selected categories with no active work, an empty project, a read failure, and a detail-load failure (centred inside the detail panel). Keep non-blocking notices inline at the top. Long diagnostics use a centred, width-limited, scrollable block. Unresolved relations stay content in their own area.

## Acceptance criteria

- [ ] the five blocking/empty states render centred
- [ ] persistence, lock/refresh, and other non-blocking notices stay inline at the top
- [ ] long diagnostics use a centred, width-limited, scrollable block
- [ ] unresolved relations are not rendered as a message
- [ ] tests cover each state and the inline/centred split
