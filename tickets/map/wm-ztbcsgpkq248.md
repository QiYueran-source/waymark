---
schema: 1
id: wm-ztbcsgpkq248
title: Plan Waymark DSH sidebar and MCP integration
category: map
status: open
owner: frank
created_at: 2026-11-04T02:01:45Z
updated_at: 2026-11-04T02:32:24Z
dependencies:
  - wm-djw788wb4606
---
## Destination

Produce an implementable functional and integration specification and a decision route for the first DSH read-only sidebar and generic access to the existing Waymark MCP server; implementation follows after this map is complete.

## Notes

- Confirmed scope and details live in the existing Waymark design map: [Waymark: General-purpose tracker design map](wm-y14hs95k8wct), DSH sidebar integration section. Decisions are recorded there immediately after confirmation.
- Sidebar uses the active DSH session workspace, retains the existing Web read-only browsing capability, and initially targets a local browser and local DSH. Missing projects and dependencies must be explained, never initialized or installed automatically.
- MCP uses the existing read/write stdio server with generic host-specific configuration guidance; DSH-specific MCP setup is not in scope.
- Keep research local; do not push or open pull requests. Do not add tracker fields solely for ticket type or claim state; parent and dependency relationships remain native Waymark relationships.
- Consult grilling and domain-modeling when working a human decision. A decision is not implementation work.

## Decisions so far

## Not yet specified

- Details of failure recovery, trust boundaries, lifecycle and UX depend on the outcomes of the host and Web investigations.
- The distribution and supported-version matrix depend on the actual supported DSH API and Waymark build artifacts.

## Out of scope

- Implementation, release and deployment of the plugin during this planning effort.
- VS Code and Cursor extensions; remote-browser support for the initial sidebar.
- Automatically downloading or installing dependencies; a second MCP server or DSH-specific MCP installer.
