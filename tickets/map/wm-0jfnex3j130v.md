---
schema: 1
id: wm-0jfnex3j130v
title: Define cross-platform lock safety contract
category: map
status: closed
owner: frank
created_at: 2026-11-05T00:53:09Z
updated_at: 2026-11-05T01:57:39Z
closed_at: 2026-11-05T01:57:39Z
parent: wm-4wbvzsv34g2p
dependencies:
  - wm-78d0sa7jaxpb
---
## Type

grilling

## Question

What safety and concurrency guarantees must the project lock provide on Windows and Unix, including symlinks/reparse points, contention, process crashes, failures and the existing Windows specification’s check-then-open race?

## Decisions confirmed so far

- Confirmed by the user on 2026-11-05 for cross-platform lock safety: if a cloned project's `.waymark/write.lock` is already a symbolic link to another file, Waymark refuses to open it, reports an error, does not modify its target, and does not continue the project write. Equivalent protection covers Windows reparse points; the later decision below rejects all types for the final lock path item, while the mechanism remains to be established. The check/open race is in scope: a separate pre-open path check alone is insufficient.
- A malicious process with permission to continuously replace the project directory, `.waymark` directory, or lock path during a write is outside this effort's security promise. This exclusion does not waive the pre-existing malicious-link rule or ordinary Waymark concurrency requirements; detectable anomalies should fail explicitly. The later decisions below specify the metadata directory, hard links, root aliases, and external lock-path replacement without implying complete adversarial-directory protection.
- Windows MSVC and GNU builds must use the same project lock protocol: simultaneous mediated writes to the same local project serialize, or the contender waits and fails under the existing timeout policy. Verify this across actual native builds. The lock is cooperative; external editors do not take it and fingerprint conflict checks still apply.

- Confirmed by the user on 2026-11-05: on Windows reject `.waymark/write.lock` when the final path item is any reparse point; do not maintain an allowlist of apparently harmless types. The safety guarantee cannot rely solely on a separate pre-open check; the mechanism requires native validation.
- Reject mediated writes if the project’s `.waymark` directory itself is a pre-existing directory link, including a Windows directory reparse point; explain that a regular metadata directory is required. This does not promise defense against malicious ancestor replacement during an operation. A later decision below permits project-root aliases while requiring them to use the same lock.
- If access, type identification, or verification of a required lock-safety condition fails, report an error and do not perform an unlocked write. Read-only operations remain lock-free and are not blocked merely because the write lock cannot be obtained.
- When a process holding the lock crashes, rely on operating-system release and bounded retry; never infer a stale lock from file existence, age, or PID, and never forcibly reclaim it on that basis. A persistent lock file alone is not evidence of a live holder. Verify Windows release and retry behavior natively.

- Confirmed by the user on 2026-11-05: a project may be accessed through a symbolic-link or equivalent alias to its root. Different aliases of the same actual project must use the same lock; native platform tests must verify this. This does not allow the `.waymark` directory itself to be a link.
- Waymark never deletes or recreates a lock file merely to release the lock and should tell users not to manually clean it. External deletion or replacement of the lock path while a write is in progress is unsupported. If the OS prevents replacement or Waymark detects an anomaly, fail safely; do not promise detection of every external replacement.
- Preserve the shared error taxonomy: lock contention that exceeds the configured bounded wait returns retryable `LOCK_TIMEOUT`; inability to safely open or verify the lock file returns `LOCK_FAILED` and the write is not automatically retried. CLI and MCP use the same classification; neither failure permits an unlocked write.
- Native Windows verification must include MSVC and GNU processes contending on the same project lock, reacquisition after a holder exits or crashes, rejection of lock-file symbolic links and reparse points without writing through them, and the required error classifications. Concrete CI provisioning and handling of unavailable test capabilities remain for the verification and release decision.

- Confirmed by the user on 2026-11-05: reject a lock file when the OS can establish that it has multiple hard links; if its hard-link count cannot be reliably checked, fail closed with `LOCK_FAILED`, without proceeding to an unlocked write. Native cross-platform feasibility and filesystem support must be validated; if a supported filesystem cannot meet the requirement, return to the user for a decision instead of silently weakening it.

## Resolution

The user confirmed the lock safety contract above on 2026-11-05. This is a design decision, not a claim that the contract is implemented or verified. Platform-specific mechanisms and hard-link inspection feasibility require native validation; if the contract cannot be met on a supported filesystem, return to the user rather than silently weakening it. Concrete CI provisioning and capability-skip policy belong to the verification and release decision.
