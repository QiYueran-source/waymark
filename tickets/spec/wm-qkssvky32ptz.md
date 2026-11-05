---
schema: 1
id: wm-qkssvky32ptz
title: Split shared and local Waymark configuration
category: spec
status: open
owner: frank
created_at: 2026-11-05T07:16:32Z
updated_at: 2026-11-05T07:16:32Z
---
## Problem Statement

Waymark currently stores the acting operator, timezone, and projection-cache settings in the shared project configuration. That file is committed and collaboratively edited, so each collaborator's operator identity and machine-specific cache preferences create needless merge conflicts and can overwrite one another. A shared timezone also does not represent each user's local display and offset-less input context.

The current configuration interfaces and adapters assume these values are project configuration. This makes local runtime preferences appear to be shared project decisions and causes different machines to compete over values that should not be part of the project baseline.

## Solution

Split configuration into shared project configuration and local user configuration. Keep project structure and data-model settings shared and reviewable. Store the acting operator, personal timezone, and cache settings in an ignored local user configuration document. Load both through one shared core configuration mechanism so CLI, MCP, and Web use identical precedence, validation, diagnostics, and effective values.

Create the local document on first project initialization with the current Git identity, machine timezone, and current cache defaults. Preserve it on repeated initialization. Allow the existing configuration-setting capabilities to target the local document for local fields, while project fields continue to update shared configuration. Continue to support manual editing.

## User Stories

1. As a collaborator, I want my operator identity stored locally, so that changing my identity does not modify the shared project configuration.
2. As a collaborator, I want another collaborator's operator identity to remain isolated from mine, so that our commits do not conflict over a local setting.
3. As a collaborator, I want the operator to be available to owner-mismatch warnings, so that Waymark can retain its advisory accountability signal.
4. As a collaborator, I want the existing creator default to use the effective operator, so that moving the operator source does not change creator provenance semantics.
5. As a collaborator, I want my timezone stored locally, so that human-readable timestamps use my local context.
6. As a collaborator, I want offset-less local time input interpreted in my configured timezone, so that my commands have the expected local meaning.
7. As a collaborator, I want my timezone change to avoid rewriting shared work-item timestamps, so that local preferences do not create project data churn.
8. As a collaborator, I want cache settings stored locally, so that machines with different resources can use different cache budgets without conflicts.
9. As a collaborator, I want cache settings to affect performance only, so that cache choices never change work-item meaning or persisted data.
10. As a new project owner, I want initialization to create a local settings document, so that the project works with sensible machine-specific defaults immediately.
11. As a returning collaborator, I want repeated initialization to preserve my local settings, so that initialization cannot overwrite my preferences.
12. As a collaborator using an existing project, I want normal initialization to retain its existing no-op behavior, so that it does not unexpectedly create or rewrite local settings.
13. As a collaborator, I want missing local settings to have safe runtime defaults, so that a missing ignored file does not prevent ordinary reads or writes.
14. As a collaborator, I want an explicit local value to override its corresponding fallback, so that I can pin a preferred identity, timezone, or cache behavior.
15. As a collaborator, I want clearing a local override to restore its fallback, so that I can return to Git, system, or built-in defaults without special sentinel values.
16. As a collaborator, I want operator fallback to use Git's effective `user.name`, so that Waymark follows the identity Git resolves for the current repository and environment.
17. As a collaborator, I want a missing Git identity to leave operator unset rather than inventing one, so that provenance and warnings are not misleading.
18. As a collaborator, I want Waymark to continue operating when Git is unavailable or the project is outside a repository, so that local tracking does not depend on Git availability.
19. As a collaborator, I want timezone fallback to use the machine-local timezone, so that a new or incomplete local configuration remains useful.
20. As a collaborator, I want timezone detection failure to fall back to UTC with an explicit informational warning, so that behavior remains deterministic and visible.
21. As a collaborator, I want cache fallback to use the built-in defaults, so that an incomplete local configuration remains valid.
22. As a collaborator, I want initialization to write the current cache defaults explicitly, so that an initial local configuration is a stable snapshot.
23. As a collaborator, I want a later program-default change not to silently alter my explicit cache snapshot, so that upgrades do not unexpectedly change machine behavior.
24. As a collaborator, I want to edit the local settings document manually, so that I do not need a new specialized workflow for simple preferences.
25. As a collaborator, I want existing setting commands to update the local settings document for local fields, so that familiar commands remain useful without modifying shared configuration.
26. As a collaborator, I want project setting commands to continue updating only project settings, so that project structure remains shared and reviewable.
27. As a CLI user, I want effective configuration output to distinguish project values from local values, so that I know what will be committed and what is machine-local.
28. As an MCP client, I want the same effective values and source metadata as the CLI, so that automation does not interpret configuration differently.
29. As a Web user, I want the Web surface to use the same core configuration resolution, so that display and time interpretation match the other adapters.
30. As a collaborator, I want user configuration validation to reject unknown or shared project keys, so that local settings cannot accidentally override project structure.
31. As a collaborator, I want malformed local configuration to produce an explicit error, so that invalid preferences are never silently ignored.
32. As a collaborator, I want validation and doctor diagnostics to include local configuration, so that I can find problems before a mediated write.
33. As a collaborator, I want local diagnostics to identify the local configuration source, so that I can repair the correct file without confusing it with project configuration.
34. As a collaborator, I want local configuration diagnostics not to change the shared revision, so that a local problem remains local.
35. As a collaborator, I want initialization to ensure the local settings document is ignored by the metadata ignore file, so that it is not accidentally added to the shared baseline.
36. As a collaborator, I want Waymark not to modify repository-level ignore files, so that initialization does not create unrelated shared changes.
37. As a collaborator, I want a missing local ignore entry repaired before a local settings write, so that newly created or updated preferences are protected from accidental tracking.
38. As a collaborator, I want an invalid, non-writable, or symlinked metadata ignore file to stop a local settings write, so that Waymark does not create a file that may be committed accidentally.
39. As a collaborator, I want a tracked local settings document to remain readable, so that a repository mistake does not make the project unusable.
40. As a collaborator, I want writes to a tracked local settings document refused, so that Waymark does not create more shared-baseline changes.
41. As a collaborator, I want the refusal to explain that I must manually untrack the local document, so that I can repair repository state deliberately.
42. As an operator, I want a one-operation explicit identity override to retain highest precedence, so that scripts and integrations can state their acting identity directly.
43. As a collaborator, I want shared core logic to load and merge configuration for every adapter, so that precedence and validation cannot drift between CLI, MCP, and Web.
44. As a maintainer, I want the old shared operator, timezone, and cache fields removed, so that there is one authoritative location for each setting.
45. As a maintainer, I want this pre-release configuration change to avoid compatibility migration, so that implementation can remove obsolete fields without preserving an unused legacy path.

## Implementation Decisions

- The domain distinguishes shared project configuration from local user configuration. Shared configuration contains the format/version metadata, storage root, categories, and custom-field declarations. Local user configuration contains operator, timezone, and cache settings.
- The local configuration has a dedicated TOML shape. Only `operator`, `timezone`, and the cache settings are accepted. Unknown keys and shared project keys are invalid.
- The core project-configuration loading boundary becomes the single seam that reads shared configuration, optionally reads local configuration, resolves local fallbacks, and exposes effective values to every adapter.
- Local precedence is: an operation-level explicit operator override; otherwise an explicit operator in the local document; otherwise Git's effective `user.name`; otherwise no operator. For timezone, an explicit local value wins, followed by the current machine-local timezone, followed by UTC when detection is unavailable. For cache, explicit local values win, followed by built-in defaults.
- The effective operator remains advisory. It is not authentication or authorization. Owner-mismatch warnings compare it with the work-item owner, and omitted creator continues to default to the effective operator when one is available.
- Git identity resolution uses Git's normal effective configuration precedence, including repository-local and broader configuration scopes. Git unavailability, absence of a repository, or inability to read `user.name` makes only that fallback unavailable; it does not fail ordinary Waymark operations when no explicit operator exists.
- First-time initialization creates the local document with the effective Git `user.name` when available, the current machine-local timezone, and the current built-in cache defaults. A missing Git name leaves operator unset. Existing local settings are never overwritten. Re-running initialization for an already initialized project remains a no-op, including when initialization operator or timezone options are supplied.
- Existing initialization operator and timezone options continue as seeds only when creating a new local document. They do not update an existing local document.
- Existing setting interfaces for operator, timezone, and cache remain available but route local fields to the local document. Project configuration operations continue to update only shared fields. Clearing a local setting removes its local override and restores its fallback; an empty operator is not a special opt-out of Git fallback.
- Local writes use the same core project lock, validation, atomic replacement, and conflict checks as shared configuration writes. They do not increment the shared configuration revision and do not modify work-item documents.
- A local document is optional during reads. Its absence does not trigger creation during ordinary reads. Runtime defaults remain available without it.
- Explicit cache values in the local document remain stable across application-default changes. Only an absent cache field follows the current built-in default.
- The local document is manually editable. A malformed document, invalid timezone, invalid cache value, empty invalid operator, unknown key, or shared project key produces an explicit configuration error rather than silent fallback.
- `config show` exposes project configuration, local configuration, effective values, and source metadata. Human output uses separate project and local sections. Machine output provides separate project, local, effective, and source structures.
- `validate` and `doctor` inspect both configuration documents. Local findings identify the local document and do not alter the shared revision or other clones.
- Initialization and local writes ensure the metadata ignore file contains the local document entry. Waymark does not modify repository-level `.gitignore` or `.git/info/exclude`.
- A missing ignore entry is repaired before local-file creation or update. An invalid, non-writable, symlinked, or otherwise unsafe ignore file causes the operation to fail before creating or updating local settings.
- A tracked local document remains readable. Initialization reports that it is tracked and does not alter the Git index. Any operation that would write it refuses and instructs the user to untrack it manually.
- The old shared operator, timezone, and cache fields are removed from the shared configuration schema. No backward-compatible migration or legacy fallback is required because the project is pre-release.

## Testing Decisions

- Tests verify externally observable configuration behavior and adapter parity, not private parsing helpers or file layout implementation details beyond the documented paths and outputs.
- Core configuration tests cover valid local documents, rejected unknown/shared keys, malformed values, absent files, explicit-over-fallback precedence, clearing overrides, cache default snapshots, timezone fallback and UTC warning behavior, Git identity fallback, Git-unavailable behavior, and removal of obsolete shared fields.
- Core initialization tests cover first initialization, initialization with Git identity and machine timezone, missing Git identity, explicit initialization overrides, repeated initialization preserving local settings, existing-project no-op behavior, and local ignore-entry creation.
- Core write-path tests cover lock and fingerprint conflict behavior for local settings, atomic replacement, revision non-increment, refusal to write tracked local settings, unsafe or unwritable ignore files, and preservation of work-item files.
- CLI integration tests follow the existing command-level patterns for init, config set/show/validate, project inspection, warnings, JSON envelopes, and error exits. They verify that local commands do not modify shared configuration and that source metadata is rendered consistently.
- MCP contract and equivalence tests follow the existing adapter-equivalence tests. They verify the same local-setting operations, effective values, source metadata, validation errors, and warnings as the CLI.
- Web tests use the shared core/view output and verify that the Web adapter does not reimplement precedence or parse local configuration independently.
- Git fallback tests use isolated repositories and configure local, global, and missing `user.name` values to verify Git's effective precedence without relying on the developer's environment.
- Timezone tests use controlled environment/system-timezone fixtures or injected platform seams where available; they verify display/input interpretation and that changing local timezone does not rewrite persisted UTC timestamps.
- Tests cover a tracked local file separately from an ignored untracked local file, because Git ignore rules do not affect an already tracked path.
- The highest preferred test seam is the shared core project configuration boundary; adapter tests remain thin parity and rendering checks over that seam.

## Out of Scope

- Authentication, authorization, or cryptographic identity verification for operator or owner.
- Changing owner semantics, creator provenance semantics, or the advisory nature of owner mismatch warnings.
- Moving storage root, categories, custom-field declarations, or format/version metadata into local configuration.
- Automatic migration of old shared operator, timezone, or cache values; backward compatibility with pre-release configuration is not required.
- A new global user configuration directory outside the project metadata directory.
- Automatic modification of repository `.gitignore`, `.git/info/exclude`, or the Git index.
- Interactive configuration editors or a new configuration UI; manual TOML editing remains supported.
- A cache persistence format or an on-disk cache/index; only cache preference settings are local.
- Synchronizing timezone or operator settings between collaborators.
- Changing persisted timestamp format or accepting caller-supplied system timestamps.

## Further Notes

This change intentionally separates project semantics from local execution context. A timezone is now a user preference: two collaborators may render the same persisted UTC instant differently, and an offset-less local input may represent different instants on different machines. Callers that require an unambiguous instant should provide an explicit offset.

The design map records the settled decisions and the remaining platform-safety constraints. This specification has been synchronized into the normative functional specification's Configuration, command-catalog, diagnostic, and error-registry sections, so that document is the single source of truth for behaviour; this specification remains the rationale and the implementation handoff for the local user configuration change.
