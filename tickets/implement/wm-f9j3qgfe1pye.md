---
schema: 1
id: wm-f9j3qgfe1pye
title: Graph canvas fills the content area and hosts unresolved relations
category: implement
status: closed
owner: frank
created_at: 2026-11-04T12:57:12Z
updated_at: 2026-11-04T14:50:18Z
closed_at: 2026-11-04T14:50:18Z
parent: wm-7k3m4p9x2cqd
---
## What to build

Let the graph use the whole content area instead of a small framed box. Keep the side detail panel: opening it shrinks the graph and refits to the remaining canvas. Move unresolved relations into a collapsible bottom strip that never overlays nodes. Resizing preserves the viewport and never auto-fits; fitting stays explicit. No full-screen mode.

## Acceptance criteria

- [ ] the graph fills the content area below the toolbar with no inner fixed-height frame or decorative border
- [ ] opening the detail panel shrinks the graph and refits to the remaining width
- [ ] unresolved relations render in a collapsible bottom strip and never overlap nodes
- [ ] window resize preserves pan/zoom; first entry and an explicit direction switch still fit
- [ ] no full-screen graph mode and no app-header hiding
- [ ] tests cover fit-on-open-detail, resize preservation, and the unresolved strip
