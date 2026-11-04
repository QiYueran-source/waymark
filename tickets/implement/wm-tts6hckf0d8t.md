---
schema: 1
id: wm-tts6hckf0d8t
title: Presentation state is isolated per project and degrades safely
category: implement
status: closed
owner: frank
creator: frank
created_at: 2026-11-04T01:04:37Z
updated_at: 2026-11-04T11:30:23Z
closed_at: 2026-11-04T11:30:23Z
parent: wm-7k3m4p9x2cqd
dependencies:
  - wm-2e3712xbs4fd
  - wm-7k80455wfcks
  - wm-9060dq38kwb2
---
## What to build

Close the state surface so one project's browsing choices can never corrupt another's, and so a hostile or absent browser store cannot break the reader's session.

Every remembered preference is namespaced by the project identity derived from the normalized project root: selected presentation, category selection, tree expansion and scroll, dependency visibility, graph collapse state, graph viewport, and the structured-alternative toggle. Preferences are kept apart by presentation, so changing the graph never rewrites the tree's state.

A store that is unreadable, unwritable, or full leaves every control working for the session, with one concise notice, and a corrupted stored value falls back to the default instead of throwing. Moving a project to another path starts that project from defaults and leaves the old namespace intact.

## Acceptance criteria

- [ ] every preference listed above round-trips, and switching presentation does not overwrite another presentation's state
- [ ] two projects on one origin keep independent state for all of them
- [ ] a project moved to a different path reads defaults rather than another project's state
- [ ] an unreadable or unwritable store leaves browsing and every control usable and shows one concise notice
- [ ] malformed stored values fall back to defaults without an exception
- [ ] nothing is written into the repository, sent to the service, or uploaded
- [ ] frontend tests cover round-trips for each preference, cross-presentation isolation, cross-project isolation, path move, unavailable storage, write failure, and malformed values
