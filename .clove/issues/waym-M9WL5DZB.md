---
schema: 1
id: waym-M9WL5DZB
title: Freeze the error taxonomy and exit-code mapping
status: open
type: feature
priority: 2
created: 2026-11-03T03:25:00.000Z
updated: 2026-11-03T03:25:00.000Z
parent: waym-VS5Z5543
labels: [wayfinder:grilling]
deps: [waym-K4QMRT7A, waym-B6TW9KPD, waym-R8YF2NCE, waym-J3VX6HQS]
relates: []
duplicates: []
supersedes: []
---
## Question

What is the complete enumerable Waymark error taxonomy, and how does each code map to a CLI exit class?

The map currently lists codes as "such as", which means the set is open and adapters cannot be written against it. Freeze:

- the full code list, including the codes newly introduced by the editing and migration decisions;
- which codes are retryable and which are terminal;
- the typed `details` payload for each code, and which `location` shapes apply;
- the mapping of codes to the seven exit classes;
- which conditions are diagnostics (findings) rather than operation errors;
- how a schema/contract test pins the set so adding a code is deliberate.
