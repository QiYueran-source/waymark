---
schema: 1
id: wm-zm3h5tcnp44t
title: Relationship lines stay distinguishable at any zoom
category: implement
status: open
owner: frank
created_at: 2026-11-04T12:57:13Z
updated_at: 2026-11-04T12:57:13Z
parent: wm-7k3m4p9x2cqd
---
## What to build

Make relationship lines legible at every zoom. Dependency dashes keep a minimum screen-space dash and gap; dependencies use a distinct colour and/or weight from parent-child lines. Hover raises and thickens an edge and shows its label. Where several dependencies share a gap, show how many and let the reader inspect them. Coincident parent-child and dependency edges are offset or haloed. The legend keeps two entries and adds the hover hint.

## Acceptance criteria

- [ ] a single dependency stays visibly dashed at every zoom, including the default fit
- [ ] dependency lines differ by colour and/or weight from parent-child lines, not dash alone
- [ ] hovering an edge raises and thickens it and shows the relation label
- [ ] multi-dependency gaps report a count and allow inspection; no default aggregation
- [ ] coincident parent-child and dependency edges keep both visible (offset or halo)
- [ ] the legend keeps both entries and adds the hover hint
- [ ] tests cover the minimum dash, hover label, coincidence, and count
