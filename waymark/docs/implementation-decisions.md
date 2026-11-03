# Implementation decisions (provisional)

The functional specification leaves some details open ("Not yet specified"). When an implementation
ticket has to pick something to compile, it records the choice here with the reason and what would
change it. These are implementation choices, not product decisions; the specification can overrule
any of them.

## Format generation marker

**Choice:** the frontmatter key `schema`, an integer, current value `1`; the configuration version
string is `"<generation>.<revision>"` (for example `"1.0"`).

**Why:** it mirrors Clove's `schema:` convention, reads well in a diff, and gives the configuration
version one obvious source for its major part.

**Open:** the specification leaves both the field name and its value shape undecided.

## List custom-field type spelling

**Choice:** a list type is written as a string `"list:string"`, `"list:integer"`, or
`"list:boolean"`.

**Why:** TOML has no union value, and a string keeps the declaration a single diffable line.

## Temporary-file mode

**Choice:** an atomic replacement inherits the destination file's existing mode; a new file is
created `0644`.

**Why:** preserving an existing mode means a project that deliberately made its documents group- or
world-readable does not silently become private on the next edit. A fixed `0644` for new files matches
what a text editor creates and keeps documents readable by every collaborator, which is the point of a
repository of Markdown files. Whatever the umask does, the mode is explicit, so behaviour does not
vary between machines.

**Open:** the specification says nothing about file modes.

## Relationship order is not stored

**Choice:** `dependencies` and `soft_links` are sorted by stable ID and de-duplicated on read as well
as on write.

**Why:** the canonical writer emits one order, so `parse(to_document(item)) == item` can only hold for
every item if the parsed value is normalized too. Treating the on-disk order as meaningful would make
the round-trip invariant conditional on how a hand edit happened to order the lines, and nothing in
the specification gives the order a meaning.

**Open:** a future ticket that gives relationships an explicit order (for example insertion order for
display) would have to store it as a separate field rather than rely on the list order.

## A duplicate frontmatter key is a parse error

**Choice:** duplicate keys are rejected by the YAML layer as `PARSE_ERROR`, not reported as a
`VALIDATION_ERROR` issue naming each duplicate.

**Why:** the document cannot be read unambiguously at all, so there is no value to validate; and
`serde_norway` refuses the document before a field-level issue list can be built. The alternative
would be a hand-written line scanner, which would duplicate the YAML parser's job to produce a
marginally better message.

## Unreadable format generation is `PARSE_ERROR` for now

**Choice:** a document whose `schema` is not `1` is rejected as `PARSE_ERROR`, with the generation
named in the message.

**Why:** the registry was frozen by a later ticket and did not contain `UNSUPPORTED_VERSION` at the
time. The reason is stated in the message so a caller is not sent hunting for a typo.

**Superseded by T2:** the registry now contains `UNSUPPORTED_VERSION`, and
`WaymarkError::unsupported_version(found, supported)` exists. The call site in `parse.rs` still raises
`PARSE_ERROR`; T13 moves it, and teaches this release to read the previous generation.

## Body line endings

**Choice:** `\r\n` in a body is normalized to `\n` on read, together with the "exactly one trailing
newline" rule.

**Why:** the store keeps one byte representation, so a CRLF checkout and an LF checkout do not produce
spurious fingerprints and diffs. The frontmatter is already line-ending tolerant.

## Default category name

**Choice:** `init` writes the category key with its first letter upper-cased (`task` → `Task`).

**Why:** the specification gives no naming convention, and a display name is required in the
configuration. The smallest transform that reads as a name is preferable to deriving one from
hyphens, which would be a visible guess. `config add category` always takes an explicit name.

## `--lock-timeout-ms 0` means fail immediately

**Choice:** a zero wait reports contention as `LOCK_TIMEOUT` without retrying.

**Why:** it gives scripts a non-blocking "is the lock free?" probe and makes the timeout policy
expressible without a second flag.

# T2 — the frozen error registry

## `USAGE_ERROR` is a CLI constant, not a registry code

**Choice:** `USAGE_ERROR` lives in `crates/waymark-cli/src/main.rs` as a constant with exit 1. It is
not a `Code`, and `Code::from_str("USAGE_ERROR")` is an error.

**Why:** no domain operation can produce a bad flag or an unknown subcommand, so a registry row for it
would let a library caller claim a recovery action that does not exist. The registry is the single
classification source for *domain* failures; the adapter owns the one failure class it invents. A test
in `tests/registry.rs` and `tests/errors.rs` pins the absence, and the CLI has its own
`adapter_failure_envelope` builder in `envelope.rs` so even this failure keeps the shared shape.

## The registry has 37 codes; the brief's "39" is a miscount

**Choice:** `Code` has exactly the 37 rows of the specification's **Error registry** table, from
`ITEM_NOT_FOUND` through `TIMESTAMP_INCOHERENT`, with the six finding-only codes included.

**Why:** the table is authoritative and the brief defers to it ("exactly the codes in the spec's Error
registry table"). Counting the table gives 37; counting it plus `USAGE_ERROR` gives 38, and the brief's
parenthetical listed the six finding-only codes correctly, so the total was simply transcribed wrong.
No code was invented to reach 39, and none was dropped: the golden snapshot names all 37 and a
consistency test fails if the enum and the snapshot disagree in either direction.

## One `Spec` row per code, read by every accessor

**Choice:** each `Code` variant maps to a private `Spec { exit, retryable, finding, details,
optional_details, locations }` through one exhaustive `match` with no wildcard. `exit_class`,
`retryable`, `details_keys`, `location_kinds`, and `finding` are all readers of that row.

**Why:** it makes the five pins one edit rather than five, and a new variant cannot compile until all
six fields are declared. The accessors are the public seam, so the row stays private and can grow later
(for example a `since_generation` field) without an API break.

## Details keys are declared, and constructors are checked against them

**Choice:** `details_keys()` returns the keys in contract order; `optional_details_keys()` returns the
subset a constructor may omit. Every typed constructor has an entry in the anti-drift test in
`tests/errors.rs`, which fails if it invents a key, omits a required one, or attaches a location kind
its code does not declare.

**Why:** "typed details" is only real if a missing key is a test failure rather than a consumer's
runtime surprise. The optional subset is data rather than a convention because exactly one code needs
it: `ITEM_NOT_FOUND` carries `{id, relation?}` — a direct lookup names no relation, a relationship
resolution does. Encoding `?` as registry metadata keeps the check strict for the other 36 codes.

## The golden snapshot is generated, then reviewed

**Choice:** `crates/waymark-core/tests/golden/error_registry.json` is a sorted array with one row per
code: `code`, `exit`, `retryable`, `details`, `location_kinds`, `finding`. Regenerate it with

```sh
UPDATE_ERROR_REGISTRY=1 cargo test -p waymark-core --test registry
```

and review the diff. The default run asserts equality and fails with a message pointing at that
command.

**Why:** the brief asks for the file to come from a test binary and to be committed. A generator that
silently rewrites on every run would let a reclassification pass unnoticed; requiring the environment
variable keeps the change visible in review, which is the point of pinning it at all.

## `ExitClass` and `Severity` are enums, not integers and strings

**Choice:** `ExitClass` has six variants with `as_i32()` returning 2 through 7; `Severity` has `error`,
`warning`, `info` and is serialized `snake_case`. `Code::exit_class()` returns
`Option<ExitClass>`, not an integer.

**Why:** `None` is exactly the finding-only case the spec describes, and the type makes "warning
changed the exit code" unrepresentable at the seam instead of merely untested. Adapters convert once,
in the CLI's single `exit_code` function.

## Version and migration codes are frozen now, unused until T13

**Choice:** `UNSUPPORTED_VERSION`, `ITEM_NEEDS_MIGRATION`, `ACTION_REQUIRES_MIGRATION`, the three
`MIGRATION_*` codes, and `ROLLBACK_FAILED` are in the registry and have constructors, but no operation
produces them yet.

**Why:** the brief freezes the taxonomy in one change, and the specification's table is the contract
regardless of which ticket first raises a code. This supersedes the earlier T1 decision that an
unreadable generation is `PARSE_ERROR`; T13 changes the call site in `parse.rs`, not the registry.

## Warnings and findings reuse the registry and cannot fail a command

**Choice:** `Warning { code, message, location }` carries a `Code`; `Finding { severity, code,
location, message, fixable }` derives its severity from `Code::finding()`. Both are built from a
finding-only `WaymarkError` through `Warning::from_error` / `Finding::from_error`, which panic on a
failure code.

**Why:** the spec says there is no second diagnostic taxonomy. Making the code typed means an adapter
cannot put an unregistered string in `_meta`, and making the severity derived means a warning's
severity cannot drift from the registry row. The constructors keep the details shape in one place
instead of in each caller.

## `field` locations name the field, not a target and a path

**Choice:** `Location::Field` serializes as `{"kind": "field", "field": "title"}`. The onboarding
notes sketched it as `Field { target, path }`; the specification only pins the *kind* and leaves the
payload open, and T1 already shipped the single-name shape.

**Why:** a field is identified by its name at the point of failure, and the surrounding error already
says which target it belongs to (`VALIDATION_ERROR.details.target`, or the work item in `location` when
the target is an item). Adding a second target/path pair would duplicate that and leave two places
naming the same subject, which is exactly the drift the registry is meant to prevent. Changing this
later is a wire break and needs its own ticket.

## Lifecycle and readiness (T5)

### `status set` is the only way a status changes

**Choice:** `ops::status::set_status(project, id, status, options)` takes a [`Status`], and
`parse_status(&str)` is the single place a literal becomes one. It accepts `open`, `in_progress`, and
`closed`, plus the two spellings `in progress` and `in-progress` that `Status::from_cli` already
documents, case-insensitively. `blocked`, `ready`, and every other status-shaped word are
`VALIDATION_ERROR` on target `status`. `start`, `close`, and `reopen` call the same function with the
argument filled in; they are not separate operations.

**Why:** `Status` has exactly three variants, so "writable blocked" is unrepresentable at the seam
rather than merely rejected. Routing the conveniences through the same function means they cannot
drift from `status set` — a bug there would have to be two bugs to stay hidden. Rejecting unknown
literals with a locatable validation issue, rather than falling back to `open`, keeps a typo from
silently reopening work.

### A repeated status is not a write, and a real change stamps `closed_at` from the operation clock

**Choice:** setting the current status returns `changed: false, written: false` and touches neither
`updated_at` nor the file. A real change sets `updated_at` **and** `closed_at` to the same instant when
entering `closed`, and clears `closed_at` when leaving it. Moving between `open` and `in_progress`
leaves `closed_at` absent.

**Why:** the spec says a repeat changes nothing, and "nothing" has to include the mtime or every
idempotent retry becomes a merge conflict. Using one instant for both fields means `closed_at` and
`updated_at` agree on the transition that created them, instead of differing by a clock read.

### `blocked_by` includes missing targets, and a closed item is never blocked

**Choice:** `Readiness { ready, blocked, blocked_by }`. An item is blocked when it is **not closed**
and at least one dependency target is not closed **or does not exist**. A missing target appears in
`blocked_by` like any other. A closed item reports `ready: false, blocked: false, blocked_by: []` even
if its dependencies are unfinished, and it never blocks anything downstream.

**Why:** a dangling target is a worse obstacle than an open one — no transition clears it — so
excluding it from `blocked_by` would make `ready` claim work can start when it cannot, which is exactly
the silent hiding the acceptance criteria forbid. And the spec is explicit that closed work is not
waiting on anything: "closed" is the answer, not "blocked by a dependency nobody will ever finish".

### Readiness lives in `readiness::Graph`, built from `&[WorkItem]`

**Choice:** `Graph::from_items(&[WorkItem])` and `Graph::from_project(&Project)` expose `children`,
`dependents`, `blocked_by`, `readiness`, `is_ready`, `is_blocked`, `path`, `edges`, and
`diagnostics`. `Readiness` is a plain value. Nothing is cached, written, or derived at parse time.

**Why:** T8's queries, T9's projection, and T10's dependency overlay all need the same computation;
putting it behind one type with one traversal means they cannot disagree, and `from_items` lets those
tickets build a graph from a projection they already have instead of rescanning. Doing it without
`petgraph` keeps the dependency list frozen and the traversal auditable — the iterative cycle walk also
means a hand-edited thousand-item chain diagnoses instead of overflowing the stack.

### Archived items are not graph nodes

**Choice:** `Graph` holds only active items. An archived item is remembered (so a reference to one is
not called a typo) but has no edges and no readiness. A dependency on an archived item therefore reads
as an obstruction and keeps the dependent blocked.

**Why:** the spec says archive removes recognized inbound relationships from active files — that is
T7's cleanup, and until it runs, an edge pointing at an archived item must not quietly count as
satisfied. Treating it as missing would be wrong too: the ID is not a typo, it is an archived
document, and `dependency add` says so with `ITEM_ARCHIVED`.

### `dependency` rejects, it never silently no-ops

**Choice:** `add_dependency` returns `INVALID_RELATIONSHIP` for a self-reference, an edge that already
exists, and any relation name other than `dependencies`; `CYCLE_DETECTED {kind: "dependency", cycle}`
with a closed path starting and ending at the source for a cycle; `ITEM_ARCHIVED` for an archived
target; `ITEM_NOT_FOUND` (with `relation: "dependencies"`) for a target that does not exist.
`remove_dependency` rejects an edge that is not there but **allows** removing an edge whose target is
missing, which is how a dangling pointer is cleaned up.

**Why:** a caller that asked to add an edge should learn it was already there — a silent success makes
"did my change apply" unanswerable. Removal is asymmetric on purpose: a dangling edge cannot be
resolved any other way, so refusing to delete it would leave the project repairable only by hand.

### `add_relationship` takes the relation name even though only one exists

**Choice:** the public entry point is `add_relationship(project, id, relation, other, options)`, with
`add_dependency` as a thin wrapper passing `"dependencies"`.

**Why:** `INVALID_RELATIONSHIP` needs a `relation` in its details, and a caller asking for `parent` or
`soft_links` here must be told the *kind* is wrong rather than silently receiving a dependency. When
T6 adds those operations they get their own validated functions, not a widened string match.

### `list`, `ready`, and `blocked` share `Filters` and report `complete`

**Choice:** `ops::list::{Filters, Listing, ListedItem, list, ready, blocked}`. `Filters` starts with
`status` and `category`; T8 adds fields to the same struct. Every read scans the whole project and
returns `_meta` with `source: memory_projection`, `completeness: complete`, and the graph's
diagnostics. `list` includes closed items and excludes archived ones; `ready` and `blocked` exclude
closed items, and a closed item is never in either.

**Why:** the spec says the three commands share one filter set, one sort vocabulary, and one
pagination model, so one struct is the shape that keeps them from diverging. `complete` is honest:
these are whole-project answers, unlike `show`, which reads one file and stays `scoped` even though it
adds derived columns from a project scan.

### `show` keeps `scoped` and adds derived columns

**Choice:** `show` still reads one document and reports `source: direct_file, completeness: scoped`. It
additionally emits `ready`, `blocked`, `blocked_by`, `relationships.children`, and
`relationships.dependents` from a whole-project graph. If that scan fails (an unparseable sibling, a
duplicate ID), `show` still succeeds: the derived fields are null and the scan failure becomes an
`INCOMPLETE_PROJECTION` diagnostic in `_meta`.

**Why:** the spec lists "derived readiness" as part of `show`'s result, but also says a failed global
operation must fail explicitly rather than return a partial graph. A direct-file read is not a global
operation, so the honest answer is the document plus "readiness unknown, and here is why" — refusing to
show an item because a different file is broken would make a corrupt sibling block all reading.

### `_meta.completeness` on a write result is `scoped`

**Choice:** status and dependency results carry `Meta::direct_file_scoped()`, i.e. `scoped`, even
though deciding a dependency edge scans the project.

**Why:** the result is one rewritten document. Reporting `complete` would claim a projection the
caller can reuse for paging, which a single document write does not produce; the scan is an
implementation detail of validation, not an artifact handed back. T9 owns the projection cache and can
revisit the wording if it starts handing one out.

## Editing (T4)

### The field change set is a sequence, and `null` is its own variant

**Choice:** `ops::edit::FieldChangeEntry { key: String, change: FieldChange }` where `FieldChange` is
`Set(FieldValue) | Clear | Null`. `update_custom_fields` takes `&[FieldChangeEntry]`, not a map.

**Why:** a `BTreeMap` would silently collapse a duplicate key, and the spec requires duplicate and
set-and-clear conflicts to be *rejected*. A sequence preserves the mistake long enough to report it,
and the operation groups by key itself. `FieldChange::Null` exists so a structured caller (JSON, MCP)
can send an explicit `null` and be told it is not a value instead of the adapter having to silently map
it onto `Clear`; it is never persisted.

### `changed_fields` lists every key whose value or presence moved

**Choice:** `FieldOutcome::changed_fields: Vec<FieldKey>` in key order, naming a key when its value or
presence differs from what was read. A `set` repeating the stored value and a `clear` of an unset key
are not changes; an empty `changed_fields` is exactly the "no effective change, no write" case.

**Why:** the spec says the operation returns "the changed field names". Reporting requested keys rather
than effective ones would claim a change that did not happen, and reporting only *sets* would hide the
clear that the caller asked for.

### One `EditOutcome` for title and body, a wider `FieldOutcome` for fields

**Choice:** `set_title` and `replace_body` return `EditOutcome { item, path, changed, written, meta }`;
`update_custom_fields` returns `FieldOutcome` with the same fields plus `changed_fields`.

**Why:** both carry the complete resulting item, the `changed` flag, the `_meta` block (so the
`OWNER_MISMATCH` warning reaches the envelope), and `written` — which differs from `changed` on a dry
run. Keeping `written` on the outcome matches `StatusOutcome` and `RelationshipOutcome`, so no adapter
has to re-derive "was this real" from the options it passed in.

### Validation runs before the lock, and the whole set before the single write

**Choice:** `validate_changes` is called before `with_project_lock`, and applies nothing until every
entry is accepted.

**Why:** a rejected change set must not hold the store lock (or wait for it) on behalf of a request
that cannot succeed, and a partially applied set would be exactly the "validates the whole change set
before one write" failure the ticket forbids. Reading the item is still inside the lock, because the
values are applied to the document that is actually committed.

### Title validation trims first, then rejects

**Choice:** the title is `trim()`ed, then rejected if empty or if the *trimmed* value contains `\n` or
`\r`. Internal whitespace, including tabs and runs of spaces, is preserved verbatim.

**Why:** trailing/leading whitespace is never part of a title, but reflowing internal whitespace would
silently change text the caller wrote. Checking line breaks on the trimmed value lets a title typed with
a trailing newline through while still making a real second line an error.

### Body canonicalization is `serialize::canonical_body`

**Choice:** `replace_body` reuses `canonical_body`, which normalizes CRLF to LF, collapses the trailing
run of newlines to exactly one, and returns zero bytes for a newline-only body. The stored value is the
canonical form, so `changed` compares canonical values.

**Why:** the writer already owns this rule and the parser applies it on read; a second copy in the edit
operation could disagree with the bytes a round-trip produces, and then every `body replace` of an
already-canonical body would look "changed".

### CLI body input: `--body` and `--body-file` are exclusive, `-` means stdin, neither means no-op

**Choice:** neither flag leaves the body unchanged (not cleared); `--body -` and `--body-file -` both
read stdin to EOF; a file is read as UTF-8 and a read failure is `IO_ERROR`. There is no interactive
editor.

**Why:** the spec says absent input leaves the body alone and an explicit empty value clears it, which
maps exactly onto "flag absent" vs `--body ""`. Deciding that `-` (in either position) means stdin keeps
the two options symmetric and keeps large bodies out of argv.

### CLI field values are parsed by declared type, never guessed from the literal

**Choice:** `integer` accepts only `-?[0-9]+` (so `+3`, `3.0`, `1_000`, `0x10` are rejected);
`boolean` accepts exactly `true`/`false`; `list:*` requires a JSON array checked element-wise;
`string` accepts a JSON string literal when the token starts with `"`, a JSON array/object is rejected,
and anything else is taken verbatim (including the empty string and embedded `=`). `KEY=VALUE` splits on
the *first* `=`.

**Why:** the spec forbids implicit conversion and says a string takes "an unambiguous token or a JSON
string literal". Splitting on the first `=` is what lets a string value contain an `=`, and rejecting a
structured token for a string field reports the mismatch at the field instead of quietly storing JSON
text in it.

### `--operator` is still not a CLI flag, so an owner mismatch can only come from the configuration

**Choice:** the editing commands do not add an operator flag; the CLI test produces `OWNER_MISMATCH` by
setting `operator` in `.waymark/config.toml` to a name that is not the item owner.

**Why:** the adapter has one way to state the acting identity today (the configuration). Adding a
per-command override is a cross-cutting accountability decision, not part of this ticket; the domain
operation already takes the override through `OpOptions`, so a later ticket can expose it without
touching `ops::edit`.
