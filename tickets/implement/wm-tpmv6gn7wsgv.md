---
schema: 1
id: wm-tpmv6gn7wsgv
title: Graph controls cluster and compact category selector
category: implement
status: closed
owner: frank
created_at: 2026-11-04T12:57:12Z
updated_at: 2026-11-04T14:50:19Z
closed_at: 2026-11-04T14:50:19Z
parent: wm-7k3m4p9x2cqd
---
## What to build

Replace inline category buttons with a compact `Categories (n)` control that opens a multi-select popover with search, select-all, clear-all, and the selected count. In the graph it belongs to the upper-left floating cluster with zoom/fit/reset and direction; the legend sits upper-right. Tree and structured list keep the same control in the top toolbar. Combined mode hides it. The popover is keyboard operable, closes on Escape or an outside click, and keeps select-all available in the filtered-empty state.

## Acceptance criteria

- [ ] the compact control shows the selected count and opens the popover with search, select-all, clear-all
- [ ] the graph control cluster holds categories, zoom/fit/reset, and direction at upper-left; the legend sits upper-right
- [ ] tree and structured list use the same control in the top toolbar with no canvas overlay
- [ ] combined mode shows no category selector; grouped mode does
- [ ] Escape and outside click close the popover; it is keyboard operable and does not obstruct the graph when closed
- [ ] the filtered-empty state offers select-all
- [ ] tests cover selection, counts, keyboard close, and mode differences
