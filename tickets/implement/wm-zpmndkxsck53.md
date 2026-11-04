---
schema: 1
id: wm-zpmndkxsck53
title: Graph node content stays inside the card and context is a badge
category: implement
status: closed
owner: frank
created_at: 2026-11-04T12:57:12Z
updated_at: 2026-11-04T14:50:18Z
closed_at: 2026-11-04T14:50:18Z
parent: wm-7k3m4p9x2cqd
---
## What to build

Every graph node keeps all content inside its card: category shape/name, state, title, blocked reason, and context badge. Text wraps and the node grows; the layout measures height. A context ancestor keeps the ordinary layout, gets a dashed outline around the whole card (no hatch or state stripe), and shows a text badge inside the card such as "Context — outside the selection"; the accessible name states the context.

## Acceptance criteria

- [ ] no graph text renders outside its node, for ordinary and context nodes
- [ ] long titles and reasons wrap and the node grows; edges and spacing account for measured height
- [ ] context nodes have the full-card dashed outline and no hatch or left state stripe
- [ ] the context badge is inside the card and wraps; the accessible name includes the context
- [ ] no context label is placed outside the node
- [ ] tests cover wrapping, measured height, and the context treatment
