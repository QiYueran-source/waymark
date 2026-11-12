---
schema: 1
id: wm-0txgrh7efs7z
title: Refuse a missing consent and a foreign partner distinctly
category: implement
status: open
owner: frank
creator: Frank
created_at: 2026-11-12T06:27:40Z
updated_at: 2026-11-12T06:27:45Z
parent: wm-62yfmh5j41t5
dependencies:
  - wm-0c5qz2jk17sx
---
## Parent

[Cross-item write consent, archive safety, and dangling references](wm-62yfmh5j41t5)

## What to build

Explain two refusals that have different recoveries. Asking to rewrite a partner that is mine without consent tells me the exact option to re-run with. Asking to rewrite a partner that belongs to somebody else is refused with or without consent, and the message says re-running cannot help — the owner has to be involved, or ownership transferred first.

## Acceptance criteria

- [ ] `link add --cascade` against another owner's item is refused and writes nothing
- [ ] `link add` without consent against another owner's item is refused and writes nothing
- [ ] The missing-consent refusal carries its own registered code and names the option to re-run with
- [ ] The foreign-owner refusal carries its own registered code and says re-running will not change the outcome
- [ ] Both codes are class 4 and not retryable in the registry, with the golden snapshot, schema, and documented table updated together
- [ ] A project with no operator, or with a malformed local user document, fails the cross-item write with a message naming which identity source is missing, never silently passing through

## Blocked by

- wm-0c5qz2jk17sx — link add records the link on the caller's side only
