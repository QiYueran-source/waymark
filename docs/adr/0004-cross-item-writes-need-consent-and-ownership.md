# Cross-item writes need explicit consent and a matching owner

A write may touch a second work item — a soft link rewrites its partner, archive strips inbound references from every other owner's documents, `create --link` rewrites the item it links to. Every such write now requires the `cascade` option and requires the second document's `owner` to equal the operator. Single-document writes to another person's item keep the existing `OWNER_MISMATCH` warning and still go through.

The alternative was to make `operator` block writes outright, the way the glossary used to describe it. That would have turned an advisory identity into a permission system without authentication behind it: the documents are plain files, so anyone can hand-edit them and the refusal would enforce nothing, while every project with no operator configured and every shared checkout would break on ordinary writes. Refusing a write that silently rewrites a colleague's `updated_at` is the part users actually want stopped; refusing an edit to a typo in someone's ticket is not.

## Considered Options

- Block every write from a mismatched operator: rejected because it enforces nothing against direct file edits, breaks shared checkouts, and turns a warning into a rule the data model cannot support.
- A `--force` flag beside `cascade`: rejected because both doors lead to the same write, and taking over another owner's item should be an owner transfer, not a bypass.
- Auto-completing a one-sided soft link on read: rejected because no read may rewrite a document, and it would move "whose edit was this" into a read.
- Cleaning inbound references during archive regardless of owner: rejected because an irreversible deletion should not silently rewrite somebody else's document, and the cleanup can be done later by the person who owns it.

Archive follows the same rule in a stronger form: without `cascade` it rewrites nothing but the archived item, and it is never refused because someone else's document still points at the target. The reference is left stale and `doctor` reports it to its owner. Derived state is deliberately left alone too — a dependency on an archived item does not make the dependent blocked — so an archive changes only the archiver's files and every other change is triggered by the person it affects. The cost is a stale reference nobody is told about except through their own `doctor` run; the alternative, rewriting other people's documents during a deletion, was judged worse.

## Consequences

- Two refusals carry different codes because they have different recoveries: a missing consent can be re-run with it, another owner's document cannot.
- A refused or non-cascading link leaves a one-sided soft link. It is reported by `show` and `doctor`, never repaired automatically, and converges on the next commit and pull.
- Ownership checks resolve the operator strictly, so an unreadable local configuration fails as a configuration problem instead of degrading into "no operator".
