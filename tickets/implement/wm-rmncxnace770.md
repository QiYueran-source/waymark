---
schema: 1
id: wm-rmncxnace770
title: Web read failure names the offending document and the fix
category: implement
status: closed
owner: frank
creator: frank
created_at: 2026-11-04T01:01:42Z
updated_at: 2026-11-04T11:30:23Z
closed_at: 2026-11-04T11:30:23Z
---
## What to build

When the read-only Web overview cannot build a complete tree, the failure must lead the reader to the fix instead of only naming INCOMPLETE_PROJECTION. Name the offending work-item document and its failing field(s) for validation findings, explain that the Web view cannot repair project data, and point at `waymark validate` as the command-line diagnostic surface. No partial tree is ever rendered.

## Acceptance criteria

- [ ] a validation finding that blocked the projection surfaces its document path and field name, not only the generic projection code
- [ ] the failure block states that the Web view is read-only and names `waymark validate` as the diagnostic entry point
- [ ] a projection blocked by unreadable documents is distinguished from one blocked by uncertain inventory paths
- [ ] no new API endpoint, envelope field, or write path is added; the UI renders existing diagnostics only
- [ ] frontend tests cover a validation-caused incomplete projection including location and guidance text, and the existing INCOMPLETE_PROJECTION test still passes
