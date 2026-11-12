---
schema: 1
id: wm-62yfmh5j41t5
title: Cross-item write consent, archive safety, and dangling references
category: spec
status: open
owner: frank
creator: Frank
created_at: 2026-11-12T06:17:10Z
updated_at: 2026-11-12T06:17:10Z
---
## Problem Statement

A single Waymark operation can rewrite work items its caller never named. Adding or removing a soft link rewrites the partner item, archiving strips inbound dependencies, parent edges, and soft links out of other people's documents, and `create --link` rewrites the partner it links to. Each of those writes also moves the other person's `updated_at`. In a shared repository that means one person's small action shows up as an edit to three of their colleagues' files, and nothing in the result says whose documents were touched. There is no authenticated identity and the operator setting is advisory, so there is no way to express "I meant this" and no way to stop it.

Two related symptoms have the same root. Closing a parent leaves its children reporting `ready` with no signal that the work above them is finished. And once items can be rewritten by their neighbours, a half-finished association (a soft link that one side recorded and the other did not) becomes an ordinary state that today is indistinguishable from a deliberate one, because nothing reports it.

## Solution

Cross-item writes become explicit and bounded. A command that would rewrite a second document requires the `cascade` option and requires that second document to belong to the operator. Without `cascade` it touches nothing but its own target; with `cascade` it rewrites only the operator's own items and never another owner's. Single-document writes to another person's item keep today's behavior: a warning, then the write.

Archive follows the same rule in its strongest form. It never rewrites another owner's document and is never refused because another owner's document still points at the target. The reference is left as it is, and the person who owns it finds it with their own `doctor` run, where the leftover appears as a dangling reference to an archived item, reported by relation kind. Derived state of other items does not change either: a dependency on an archived item does not make its holder blocked, and an orphaned subtree keeps reporting exactly what it did before. `tree` already shows a live item whose parent is archived as unresolved, so the orphan stays visible without becoming a root.

Soft links keep their two-sided storage but gain a one-sided reality. Without `cascade`, `link add` records the link on my side only, `link remove` drops it from my side only, and both report that the other side was left alone. `doctor` reports the one-sided link to both sides as a fact, never as a runnable repair command, and the pair converges when each side runs its own command. `show` reports it too.

A closed parent stays a read signal: active children of a `closed` item are labelled as having a closed parent in `list`, `ready`, and `doctor`, and nothing is written.

## User Stories

1. As a contributor, I want a write that only changes my own item to keep working exactly as before, so that the new consent rule never gets in the way of ordinary editing.
2. As a contributor, I want a write to another person's item to keep producing an `OWNER_MISMATCH` warning and still succeed, so that shared checkouts and projects with no operator configured keep working.
3. As a contributor, I want `link add` without `cascade` to record the link on my item only, so that adding a link never rewrites someone else's file.
4. As a contributor, I want `link add --cascade` to write both sides when the partner is mine, so that I can complete a link in one step when I own both items.
5. As a contributor, I want `link add --cascade` to be refused when the partner belongs to someone else, so that consent alone cannot grant me access to their document.
6. As a contributor, I want `link add` without `cascade` to be refused when the partner belongs to someone else, so that I cannot half-link into a document that will never record it.
7. As an agent, I want the refusal for a missing consent to tell me the exact option to re-run with, so that I can correct my own request without guessing.
8. As an agent, I want the refusal for a foreign partner to tell me that re-running cannot help, so that I stop retrying and involve the owner instead.
9. As a contributor, I want `link remove` without `cascade` to succeed by removing only my side, so that disconnecting a link is always possible without someone else's cooperation.
10. As a contributor, I want `link remove --cascade` to remove both sides when the partner is mine, so that a link I created on both sides can be undone in one step.
11. As a contributor, I want every command that rewrote a second document to list the documents it wrote, so that I can see the full footprint of my action after the fact.
12. As a contributor, I want `create --link` without `cascade` to record the link on the new item only, so that creating an item never edits the item it references.
13. As a contributor, I want `create --link --cascade` to rewrite a partner that is mine, so that a new item can arrive with both sides of the link in place.
14. As a contributor, I want `create --link` to be refused when the partner belongs to someone else, whatever I pass, so that the consent flag never becomes a way around ownership.
15. As a contributor, I want `create --link` to report the partner it rewrote when it did rewrite one, so that the partner's `updated_at` change is explained somewhere.
16. As a contributor, I want `archive` without `cascade` to rewrite nothing but the archived item, so that retiring my work never edits a colleague's file.
17. As a contributor, I want `archive` to succeed even when other people's items depend on, soft-link to, or are parented by the item I archive, so that I am never blocked by somebody else's bookkeeping.
18. As a contributor, I want `archive` to tell me which other-owner items still reference the archived item, so that I know whose attention to ask for.
19. As a contributor, I want `archive --cascade` to clean up inbound references on my own items only, so that I can keep my own graph tidy without touching anyone else's.
20. As a contributor, I want `archive` to keep refusing when the item still has an unarchived child, so that the hierarchy rule is unchanged by the new consent rule.
21. As a contributor, I want the archived item's own relationship fields to stay as they are, so that the record of what it referenced survives archival.
22. As a contributor, I want a dependency on an archived item to leave my item's `ready` and `blocked` exactly as they were, so that nobody else's archive silently changes my queue.
23. As a contributor, I want a parent edge to an archived item to leave my item's lifecycle state alone, so that archiving someone's epic does not close or reopen my work.
24. As a contributor, I want a soft link to an archived item to stay in place, so that the association remains visible as history rather than disappearing.
25. As a contributor, I want `tree` to keep showing a live item whose parent is archived as an unresolved node with an archived-parent reason, so that an orphaned subtree is visible rather than promoted to a root.
26. As a contributor, I want `doctor` to report a dependency that points at an archived item, so that I can repoint or drop it myself.
27. As a contributor, I want `doctor` to report a parent that points at an archived item, so that I can move my item under a live parent.
28. As a contributor, I want `doctor` to report a soft link whose partner is archived, so that I can drop the stale association.
29. As a contributor, I want `doctor` to report a soft link that exists on one side only, separately from one whose partner is archived, so that I can tell "the partner never recorded it" from "the partner is gone".
30. As a contributor, I want `show` to report a one-sided soft link, so that I notice the asymmetry when I look at the item rather than only when I run a full diagnostic.
31. As a contributor, I want `doctor` findings to state the fact without offering a command, so that nobody — human or agent — reads a diagnosis as permission to run a repair on someone else's document.
32. As a contributor, I want `doctor` findings grouped by relation kind, so that the repair I choose matches the kind of reference I am fixing.
33. As a contributor, I want a summary line from `archive` naming how many other-owner items still reference the archived item, so that I can judge whether to ask anyone before I finish.
34. As a contributor, I want `archive --dry-run` without `cascade` to report exactly the documents it would leave untouched, so that the preview matches what the real run does.
35. As a contributor, I want `archive --dry-run --cascade` to report which of my documents it would rewrite, so that I can review the footprint before committing to it.
36. As a contributor, I want `--dry-run` to never promise a write the real run would refuse, so that the preview stays trustworthy.
37. As a maintainer, I want closures to keep not cascading to children, so that hierarchy and lifecycle stay independent.
38. As a contributor, I want a child of a closed parent to be labelled as such in `list` and `ready`, so that I can see the parent finished before I start work.
39. As a contributor, I want that label to be derived and stored nowhere, so that no file changes when a parent closes.
40. As a contributor, I want only the direct parent to be inspected for that label, so that the signal is cheap and never depends on a long ancestor walk.
41. As a contributor, I want a closed child under an open parent to produce no signal, so that the normal case of a parent being closed last is not flagged.
42. As an integrator, I want the MCP tools to expose the same consent option with the same meaning, so that an agent has no weaker route than the CLI and no stronger one.
43. As an integrator, I want the CLI and MCP to write byte-identical documents for the same cross-item operation, so that I can rely on the adapter choice not changing the repository.
44. As a project maintainer, I want a cross-item write to fail clearly when no operator resolves, so that I fix my identity rather than silently bypassing the rule.
45. As a project maintainer, I want a broken local user configuration to fail a cross-item write as a configuration error, so that it does not degrade into "no operator" and let the write through.
46. As a contributor, I want a `--force` flag never to be accepted for these operations, so that there is exactly one way to write another owner's document: becoming its owner.
47. As a contributor, I want to transfer ownership first and then use `cascade` normally, so that taking over an item is an explicit, recorded act.
48. As a contributor, I want a non-cascading write that rewrote nothing to report nothing, so that ordinary results stay quiet.
49. As a contributor, I want `doctor` to remain read-only for business data under this change, so that a diagnostic run can never repair a stale reference on my behalf.

## Implementation Decisions

- **One consent flag.** `cascade` is a boolean on `OpOptions` and reaches only the operations that can touch a second document: `link add`, `link remove`, `archive`, and `create` with soft links. `dependency`, `parent`, and single-document edits take no such option, and their signatures do not grow one.
- **Two conditions, both required.** A second document is rewritten only when (a) its `owner` equals the effective operator and (b) `cascade` was given. `creator` never participates. Both are checked before any staging, under the project lock.
- **Refusals are registry codes, not prose.** `CASCADE_CONSENT_REQUIRED` (class 4, not retryable, details: the operation, the caller's item, the other item) and `OWNED_BY_OTHER` (class 4, not retryable, details: the other item's id and owner), both forming the recovery-unit split the registry demands. The class-4 rule — conflict and state rejection — covers both, since neither can proceed as issued. The golden registry snapshot, the JSON schema, and the documented table change together.
- **`link remove` is the asymmetry.** Without `cascade` it removes only the caller's side and succeeds; it skips the ownership question entirely because it writes one document. With `cascade` the existing two-sided path runs under the ownership check. `link add` has no such asymmetry: without `cascade` it records the caller's side and reports the partner as unsynchronised, which is the only reading that keeps "adding is a claim about both" true without a write.
- **`create --link` is judged on three identities.** The new item's `owner`, the partner's `owner`, and the operator must agree before the partner is rewritten; otherwise the partner is left alone. This closes the existing gap where a partner was rewritten and the result carried no warning at all.
- **Archive splits target from cleanup.** The target is always written. Inbound cleanup is attempted only with `cascade`, and then only against documents owned by the operator; a foreign document is skipped and named in the result. The operation is never aborted because a foreign document references the target. `HAS_ACTIVE_CHILDREN` is untouched.
- **Archive outcome gains an untouched list.** `ArchiveOutcome` carries, besides the existing removals and affected documents, the documents it left alone and the relation that referenced the target, so the adapters can render "left for its owner" without re-reading the project.
- **Derived state is frozen across archive.** No change to `blocked_by`, `readiness`, or `status` computation in this change. A dependency on an archived target continues to be treated as a known target and therefore non-blocking; the archive does not flip anybody's `blocked`. The mismatch is reported by `doctor` only.
- **`doctor` gains relation-kind findings.** The existing archived-target check is extended and re-worded so each item reports at most one finding per relation kind (`dependencies`, `parent`, `soft_links`) pointing at an archived target, and the one-sided soft link gets its own registry code so callers can tell the two apart. Neither carries a suggested command and neither is auto-fixed.
- **Closed parent is a row signal.** The label is computed on the list-family row and rendered where the readiness column is rendered, driven by a graph lookup of the direct parent; it is never persisted and never changes readiness.
- **Adapters stay thin.** The CLI and MCP both construct `OpOptions` with the new field; MCP calls it `cascade` with identical semantics. Rendering of untouched documents and one-sided links lives in the shared view layer so both adapters show the same thing.
- **Operator resolution is strict for ownership.** The ownership comparison uses the strict configuration path, not the lenient one that warning generation uses, so a malformed local user document is a `CONFIG_INVALID` rather than a silent "no operator".

## Testing Decisions

A good test drives the public operation against a temporary project and reads the documents back, asserting which bytes changed and which did not. Tests must not assert on internal helper calls; the promise of this feature is about which files move and which stay identical, so the byte-level check is the assertion.

The existing seams are sufficient and no new one is needed: the domain operations in the core crate's `ops` modules, the doctor report, and the collection reads. The feature is covered at exactly those three seams plus two adapter seams that already exist.

- **Domain seam (`ops::link`, `ops::archive`, `ops::create`, `ops::doctor`, `ops::list`).** Prior art: the archive suite already drives the public API, reads documents back, and asserts untouched files keep their exact bytes and their `updated_at`; the link suite already checks both documents change in one batch. Extend those files rather than adding new ones. The cases that must exist: a foreign document's bytes and `updated_at` are untouched after archive without `cascade`; a foreign reference does not abort the archive; `cascade` rewrites only the operator's own inbound documents; both refusal codes come back with their typed details; `link remove` without `cascade` succeeds and leaves the partner byte-identical; a dependency on an archived item leaves `ready` and `blocked` unchanged; the list row reports a closed parent and a closed-parentless row does not.
- **Doctor seam.** Prior art: the doctor suite already asserts that findings use the registry shape and that missing and archived targets are both dangling references. Extend the archived-target case to assert one finding per relation kind, the distinct one-sided-link code, and that no finding carries a repair command.
- **CLI seam.** Prior art: the CLI test suites run the binary and parse the JSON envelope. Cover `--cascade` and its absence for each of the four commands, the exit class of both refusals, the touched/untouched document lists, and that `--dry-run` output matches what the real run then does on the same project.
- **MCP seam.** Prior art: the MCP equivalence suite copies one project, drives the same operation through both adapters, and compares every byte with operation-clock fields normalized. Extend it to the four cross-item operations with and without `cascade`, and extend the parity suite so the new option is covered by name in both surfaces.

## Out of Scope

- Any change to the `ready`, `blocked`, or `blocked_by` definitions. A dependency on an archived item keeps its current non-blocking treatment.
- A `--force` flag, a permission model, or authenticated identities.
- Automatic repair of any kind: no read rewrites a document, no background process exists, and `doctor` stays read-only for business data.
- Any mechanism for notifying another person outside their own `doctor` run: no event log, no mail hook, no network call.
- Web view changes. The Web is read-only over the same derived data; a closed-parent label there is a separate decision.
- Retrofitting the consent rule onto operations that write only their own target.
- Any change to command grouping or top-level CLI command organization.

## Further Notes

- The design record is the "Cross-item write scope and the `--cascade` consent flag" section of the general-purpose tracker design map [Waymark: General-purpose tracker design map](wm-y14hs95k8wct), and the reasoning behind the shape is recorded in `docs/adr/0004-cross-item-writes-need-consent-and-ownership.md`. The glossary additions — Cross-item write, Consent, Closed parent — are already in `GLOSSARY.md` and use that vocabulary here.
- The one-sided soft link turns "the stored set differs between two documents" into normal state. Anything that assumes symmetry today has to be checked when this is implemented; the design deliberately does not add a reconciliation step, because reconciliation would mean one side repairing the other.
- Implementers should expect existing tests that assert an archive cleans inbound edges from other owners' documents to change; those assertions encode the old behavior and move to the `cascade` cases.
- The closed-parent label is the only user-visible change in this spec that does not involve a write; it is included because it addresses the same complaint — an operation feels silently inconsistent — and it costs no file change.
