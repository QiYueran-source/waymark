---
schema: 1
id: wm-xb5jkje7t4c4
title: Unify cross-item write behaviour on leave-and-report
category: implement
status: closed
owner: frank
creator: Frank
created_at: 2026-11-12T11:36:09Z
updated_at: 2026-11-12T11:49:36Z
closed_at: 2026-11-12T11:49:36Z
parent: wm-62yfmh5j41t5
---
## Parent

[Cross-item write consent, archive safety, and dangling references](wm-62yfmh5j41t5)

## What to build

The nine tickets of this spec shipped three different answers to one question — what happens when a cross-item write would have to rewrite a document the operator may not write. `link add` refuses, `archive` and `create --link` leave the document alone and report it, and `link remove --cascade` refuses. This ticket unifies the rule on leave-and-report, the shape the design map described from the start, and removes the spec's self-contradiction that produced the divergence.

Rule to implement:

- Without consent, `link add` records the link on the caller's document only and reports the partner as unsynchronised. It no longer refuses. A one-sided soft link is a normal, reported state, exactly as `create --link` and `link remove` already behave.
- With consent and a partner the operator owns, `link add`, `link remove` and `create --link` rewrite both sides as they do today.
- With consent and a partner owned by somebody else, `link add` and `link remove` still refuse with `OWNED_BY_OTHER`: the caller explicitly asked to write the other document, and silently downgrading that request would make a two-sided removal look complete when it is not.
- With consent and a foreign partner, `create --link` leaves the partner alone and reports it. Creating is not a request to write the partner.
- `archive` keeps its current behaviour: never refused for a foreign reference, leaves it and names it.
- `CASCADE_CONSENT_REQUIRED` becomes unreachable for the link verbs; decide whether to retire it or keep it registered for future cross-item operations, and record the decision.

## Acceptance criteria

- [ ] `link add` without consent succeeds, writes only the caller's document, and reports the partner as unsynchronised; a repeat changes nothing
- [ ] A repeated non-consenting `link add` still changes nothing and does not move a timestamp
- [ ] `link add --cascade` and `link remove --cascade` against a foreign partner are still refused with `OWNED_BY_OTHER`, and neither writes a byte
- [ ] `create --link --cascade` against a foreign partner still leaves it alone and names it
- [ ] Every cross-item operation's behaviour with and without consent is described in one table in docs/implementation-decisions.md, and the table matches what the tests assert
- [ ] The CLI and MCP still produce byte-identical results for every case
- [ ] The spec's user stories no longer contradict its implementation decisions

## Blocked by

None (can start immediately).
