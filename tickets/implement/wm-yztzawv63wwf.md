---
schema: 1
id: wm-yztzawv63wwf
title: Git availability degrades cleanly with a dedicated finding
category: implement
status: open
owner: frank
created_at: 2026-11-04T12:57:12Z
updated_at: 2026-11-04T12:57:12Z
parent: wm-srxj8ztv1zfz
---
## What to build

Git is optional: when no `git` executable is on `PATH`, when no repository contains the project, or both, `history` and `diff` must succeed with empty results and informational findings, and `doctor`/`validate` must report the same environment state. Add the finding-only `GIT_UNAVAILABLE` code beside the existing `NOT_A_GIT_REPOSITORY`, one finding per missing cause.

## Acceptance criteria

- [ ] registry has 38 codes; `GIT_UNAVAILABLE` is finding-only, info severity, non-fixable, location none, details `{}`
- [ ] the compile-time match, registry-consistency test, JSON-schema enum, and golden snapshot pin the new code
- [ ] history and diff succeed with empty payloads and info findings for missing executable, missing repository, and both
- [ ] the two findings are distinct and each message names its cause
- [ ] doctor and validate report Git availability as info, non-fixable findings, and never install Git or initialize a repository
- [ ] exit code stays 0 for these cases in CLI and MCP
