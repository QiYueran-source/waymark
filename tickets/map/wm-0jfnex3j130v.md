---
schema: 1
id: wm-0jfnex3j130v
title: Define cross-platform lock safety contract
category: map
status: open
owner: frank
created_at: 2026-11-05T00:53:09Z
updated_at: 2026-11-05T01:54:13Z
parent: wm-4wbvzsv34g2p
dependencies:
  - wm-78d0sa7jaxpb
---
## Type

grilling

## Question

What safety and concurrency guarantees must the project lock provide on Windows and Unix, including symlinks/reparse points, contention, process crashes, failures and the existing Windows specification’s check-then-open race?

## Decisions confirmed so far

- Confirmed by the user on 2026-11-05 for cross-platform lock safety: if a cloned project's `.waymark/write.lock` is already a symbolic link to another file, Waymark refuses to open it, reports an error, does not modify its target, and does not continue the project write. Equivalent protection covers relevant Windows reparse points; the precise types and mechanism remain to be established. The check/open race is in scope: a separate pre-open path check alone is insufficient.
- A malicious process with permission to continuously replace the project directory, `.waymark` directory, or lock path during a write is outside this effort's security promise. This exclusion does not waive the pre-existing malicious-link rule or ordinary Waymark concurrency requirements; detectable anomalies should fail explicitly. Ancestor, hard-link, and path replacement details remain to be specified without implying complete adversarial-directory protection.
- Windows MSVC and GNU builds must use the same project lock protocol: simultaneous mediated writes to the same local project serialize, or the contender waits and fails under the existing timeout policy. Verify this across actual native builds. The lock is cooperative; external editors do not take it and fingerprint conflict checks still apply.

## Still to decide

Determine exact Windows reparse-point coverage and handling, lock-path stability under ordinary concurrency, failure semantics and native verification; distinguish trusted directory assumptions from unsupported adversarial mutation. This ticket remains open.
