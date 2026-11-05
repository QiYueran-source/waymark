---
schema: 1
id: wm-dq6f226w3g5k
title: Define safe temporary and recovery file diagnostics
category: map
status: closed
owner: frank
created_at: 2026-11-05T02:09:09Z
updated_at: 2026-11-05T05:45:10Z
closed_at: 2026-11-05T05:45:10Z
parent: wm-4wbvzsv34g2p
dependencies:
  - wm-a3ewqxebssk1
---
## Type

grilling

## Question

How do diagnostics identify and report possible temporary and recovery material after interruption without misclassifying active files, following untrusted links, modifying files, or treating them as authoritative? Define scope and behavior across supported platforms.

## Decisions confirmed so far

- Diagnostic checks for possible temporary or recovery material are bounded to known, safely accessible locations; they do not recursively scan the entire project or system to guarantee discovery. Report the checked scope and coverage that cannot be confirmed. A migration interruption may leave material in a former location no longer known from the current configuration, so no findings do not prove that no leftovers exist.

- Confirmed by the user on 2026-11-05: the candidate locations are derived entirely from the current configuration — the configured storage root, each configured category directory, and the project metadata directory — not from hard-coded platform paths and not from the system temporary directory. This follows from the agreed rule that staging happens in the destination directory, so temporary and recovery material can only be produced there. The rule is identical on every platform; only the amount of coverage actually achieved differs. A location no longer derivable from the configuration, such as a former category directory after an interrupted migration, is reported as uncovered rather than silently assumed clean.
- Confirmed by the user on 2026-11-05: an inspection that cannot reach a definite answer is recorded as unconfirmed, never as absent. This covers a file another process holds exclusively open on Windows, a directory that cannot be enumerated for permission reasons, and the equivalent permission-denied or path-disappeared races on Linux and macOS. A single unreachable location yields one coverage-gap record and does not fail the whole diagnostic; the diagnostic keeps checking the locations it can reach.
- Confirmed by the user on 2026-11-05: results are carried by the existing diagnostic output and the existing finding structure instead of a new command or a new entry point. Two read-only codes are added: one for material that may require human inspection and one for a scope that cannot be confirmed. Both carry a path and a reason, use info or warning severity, and never change the exit code; the existing `--strict` rule that turns remaining error-severity findings into a non-zero exit is unaffected because neither new code is error severity.
- These rules do not make leftover material authoritative and do not promise automatic crash recovery. Diagnostics still do not delete, move, restore, or follow a link to read contents, and they still do not infer that material is abandoned from its name or age.

## Resolution

The user confirmed the diagnostic scope, uncertainty handling and result carriage above on 2026-11-05. Candidate locations are configuration-derived rather than platform-hard-coded, unconfirmed coverage is reported instead of being treated as clean, and findings reuse the existing structure with two new read-only codes. Native behaviour of the individual inspection primitives, including which Windows sharing and permission conditions actually block an inspection, remains for the platform module and verification work; no specification synchronization is authorized here.

## Handoff

Concrete inspection primitives, the exact set of conditions that count as unconfirmed on each platform, and native test evidence belong to [Define platform adaptation module design](wm-rn893kzbg524.md) and [Define cross-platform verification and release gates](wm-9ghaw0aqapyf.md).
