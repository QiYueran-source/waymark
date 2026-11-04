---
schema: 1
id: wm-jf7frx1k3jkz
title: Graph direction switch between top-to-bottom and left-to-right
category: implement
status: open
owner: frank
created_at: 2026-11-04T12:57:12Z
updated_at: 2026-11-04T12:57:12Z
parent: wm-7k3m4p9x2cqd
---
## What to build

Add a graph-only direction control between top-to-bottom (default) and left-to-right. Switching changes layout only and preserves category selection, selected item and detail panel, graph expansion, and dependency visibility. Fit the visible graph after a switch, to the remaining canvas when the detail panel is open. Persist one preference per project; on storage failure keep switching usable for the session with the existing notice. Restore the saved viewport on refresh and fit on first entry. Direction is explicit only and keyboard operable, with the selected direction exposed to screen readers.

## Acceptance criteria

- [ ] the graph toolbar offers top-to-bottom and left-to-right; the tree shows no direction control
- [ ] first use is top-to-bottom; the per-project preference survives refresh and does not leak across projects
- [ ] switching preserves selection, detail, collapse, dependency visibility, and sibling order
- [ ] after switching the visible graph fits the remaining canvas (full canvas when detail is closed), with pan/zoom afterwards
- [ ] refresh restores the saved viewport for the current direction; no usable record falls back to fit
- [ ] direction is never chosen automatically; storage failure keeps switching usable and shows the persistence notice
- [ ] the control is keyboard operable and exposes the selected direction to screen readers
- [ ] tests cover both directions, preservation, fit, restore, and storage failure
