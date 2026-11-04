---
schema: 1
id: wm-djmdzgybexeh
title: CLI work-state colours for a terminal
category: implement
status: closed
owner: frank
created_at: 2026-11-04T12:57:12Z
updated_at: 2026-11-04T14:50:18Z
closed_at: 2026-11-04T14:50:18Z
parent: wm-srxj8ztv1zfz
---
## What to build

Add colour to human CLI output without changing its text layout: a global `--color=auto|always|never` flag (default `auto`), honour `NO_COLOR`, and under `auto` colour only when stdout is a terminal. Reuse the shared display state and the Web's green/blue/red/grey mapping; colour never carries meaning alone, and category identity stays shape-based.

## Acceptance criteria

- [ ] `--color=never` and non-TTY output are byte-identical plain text to today
- [ ] `NO_COLOR` disables colour; `--color=always` forces it; `auto` colours only a TTY
- [ ] ready/in_progress/blocked/closed use the same semantic colours as the Web (ANSI approximation)
- [ ] colour never changes the words; state text and the blocked marker remain
- [ ] `show --raw`, JSON envelopes, and raw `git diff` patches stay uncoloured
- [ ] CLI tests cover the flag matrix and strip-ANSI text equality
