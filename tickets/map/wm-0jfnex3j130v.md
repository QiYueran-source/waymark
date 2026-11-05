---
schema: 1
id: wm-0jfnex3j130v
title: Define cross-platform lock safety contract
category: map
status: open
owner: frank
created_at: 2026-11-05T00:53:09Z
updated_at: 2026-11-05T01:55:30Z
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

- Confirmed by the user on 2026-11-05: on Windows reject `.waymark/write.lock` when the final path item is any reparse point; do not maintain an allowlist of apparently harmless types. The safety guarantee cannot rely solely on a separate pre-open check; the mechanism requires native validation.
- Reject mediated writes if the project’s `.waymark` directory itself is a pre-existing directory link, including a Windows directory reparse point; explain that a regular metadata directory is required. This does not promise defense against malicious ancestor replacement during an operation. Whether the project root itself can be accessed through a link remains open.
- If access, type identification, or verification of a required lock-safety condition fails, report an error and do not perform an unlocked write. Read-only operations remain lock-free and are not blocked merely because the write lock cannot be obtained.
- When a process holding the lock crashes, rely on operating-system release and bounded retry; never infer a stale lock from file existence, age, or PID, and never forcibly reclaim it on that basis. A persistent lock file alone is not evidence of a live holder. Verify Windows release and retry behavior natively.

## Still to decide

Determine project-root link policy, hard-link and lock-path stability under ordinary concurrency, contention and failure semantics, and native verification. This ticket remains open.
