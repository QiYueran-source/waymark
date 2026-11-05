---
schema: 1
id: wm-xk0vkkc45wbx
title: MCP exposes the same values and sources
category: implement
status: open
owner: frank
created_at: 2026-11-05T09:02:56Z
updated_at: 2026-11-05T09:02:56Z
---
## Parent

[Split shared and local Waymark configuration](wm-qkssvky32ptz)

## What to build

An automation client sees exactly the configuration a person sees through the CLI, including where each effective value came from.

`waymark_config_read` returns the shared configuration, the local configuration, the effective values, and the per-field sources, so a client never has to guess which document won. `waymark_config_validate` returns the findings for both documents. `waymark_project_info` no longer claims to report an operator or a time zone as project data. The setting tools route the local scalars to the local document with the same revision neutrality as the CLI.

The MCP adapter keeps calling the one shared core boundary and parses no local document of its own, so CLI and MCP values cannot diverge.

## Acceptance criteria

- [ ] `waymark_config_read` returns separate project, local, effective, and source structures in the shared envelope.
- [ ] `waymark_config_validate` returns findings for both documents with their locations.
- [ ] Setting a local scalar through MCP writes the local document and leaves the shared revision unchanged.
- [ ] `waymark_project_info` reports no operator or time zone as project data.
- [ ] The MCP adapter contains no second precedence or local-document parsing path.
- [ ] Adapter-equivalence tests assert identical effective values, sources, errors, and warnings across CLI and MCP.

**Blocked by:** wm-6fh9j35fkwmb — local user configuration document and effective resolution.

**Status:** ready-for-agent
