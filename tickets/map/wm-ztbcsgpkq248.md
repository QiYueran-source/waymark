---
schema: 1
id: wm-ztbcsgpkq248
title: Plan Waymark DSH sidebar and MCP integration
category: map
status: open
owner: frank
created_at: 2026-11-04T02:01:45Z
updated_at: 2026-11-04T11:56:11Z
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

- [Investigate DSH sidebar extension and session context](wm-n35k9b95d312): Installed DSH 0.2.1-rc.5 provides a session-scoped right-sidebar tab registry, session workspace lookup through `useSessions`, and generated host remotes; an external plugin's own Remote build recipe, live availability and end-to-end behaviour remain unproven.
- [Investigate Waymark MCP and Web runtime prerequisites](wm-rb5kkje5001d): MCP is an existing stdio `waymark-mcp`; Web is a development-only launcher with external assets, no `waymark serve`, no embedded assets and no distribution artifacts; the loopback/Host/Origin boundary and build prerequisites are verified.
- [Investigate safe reuse of Waymark Web in DSH](wm-v9qm3qxkse52): A separate-origin iframe is the least invasive reuse but is unproven and currently unfenced (no CSP/frame-ancestors, no authentication); a host-side read-only adapter is the fallback if the embedding boundary cannot be established.

## Not yet specified

- The supported DSH version range and compatibility-preflight policy: the research established the installed `0.2.1-rc.5` only, and DSH offers no cross-release API/ABI guarantee, so the supported matrix cannot be fixed until a target release policy is chosen.
- How the sidebar obtains a runnable Waymark Web service and its assets, given that only the development launcher exists and formal packaging/distribution is a separate, not-yet-created effort.

## Out of scope

- Implementation, release and deployment of the plugin during this planning effort.
- VS Code and Cursor extensions; remote-browser support for the initial sidebar.
- Automatically downloading or installing dependencies; a second MCP server or DSH-specific MCP installer.
