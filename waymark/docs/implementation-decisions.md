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

## Configuration surface (T3)

### A configuration write compares content, not the revision counter

**Choice:** every configuration write takes the project lock, re-reads `.waymark/config.toml` from disk
*inside* the lock, and decides "changed" by comparing the canonical serialization of the document it
would write against the canonical serialization of the document it read. The revision is incremented by
exactly one only when those bytes differ; a hand edit that left the counter alone is still seen.

**Why:** the spec makes the revision informational and fingerprint-based change detection authoritative.
Comparing whole documents is the strictest form of that rule and needs no separate fingerprint file; it
also makes a no-op write structurally impossible.

**Open:** if configuration ever grows large enough that canonical serialization is expensive, the same
rule can be restated as a fingerprint comparison without changing the outcome.

### `CONFIG_INVALID` findings are produced from the same validator as the error path

**Choice:** the parser was split into a `build` step that returns `(Config, Vec<ConfigIssue>)` and a
public `Config::issues_in_file` / `Config::issues_at`. `Config::load` renders the issue list into the
aggregate `CONFIG_INVALID` error it always did; `validate_config` turns one issue into one
`CONFIG_INVALID` finding with a `config_key` location. A document that is not parseable TOML at all is
one finding, not a per-key list.

**Why:** the spec requires per-issue, locatable findings and the existing single aggregate error could
not carry a location per key. One validator keeps the two representations from drifting, which is the
failure mode a second hand-written checker would have.

### `config validate` and `validate` exit 3 when findings are errors

**Choice:** a validation run that finds `CONFIG_INVALID` issues returns the `CONFIG_INVALID` error, so
the registry's invalid-input exit class (3) applies and `--json` prints the standard failure envelope.
A clean run exits 0 with `{"valid": true, "findings": []}`. There is no `--strict` flag in this release.

**Why:** a caller scripting `waymark config validate && …` must not see success on an invalid
configuration. The spec's strict-mode switch belongs to the diagnostics ticket; until then, errors are
errors and there is no way to run a non-failing check.

**Open:** T14 may add `--strict` and a warning-only default; if it does, this entry is superseded.

### `validate` covers configuration only, and says so

**Choice:** `waymark validate` reports configuration findings today, and its human output and JSON
`scope: ["configuration"]` state that work-item diagnostics are not part of this release. It is not a
stub that silently reports "all good".

**Why:** the spec's project-wide `validate` includes item-level diagnostics owned by the diagnostics
ticket. Reporting a clean bill of health for checks that never ran would be worse than a partial answer
that names its scope.

### `config add category` creates the category directory

**Choice:** after the configuration write succeeds, the category directory is created with
`create_dir_all`. A dry run creates nothing, and an already existing directory is not an error.

**Why:** `init` creates every configured category directory and `create` writes into it without creating
it; without this, `config add category` would produce a configuration that cannot be used until someone
runs `mkdir`. Creating a directory is not a work-item write, so the "configuration never touches work
items" rule is untouched.

**Open:** directory creation happens after the configuration replacement, so a failure there leaves a
declared category without its directory. Creating it first would leave an orphan directory instead; the
declaration is the authority, so the current order is the recoverable one.

### `remove_category` refuses with `VALIDATION_ERROR`, not a new code

**Choice:** a used category is refused with `VALIDATION_ERROR` whose `details.issues` has one entry per
item found in the category, each naming the item's stable ID and path. No work-item file is opened for
writing, and none is modified. No `CATEGORY_IN_USE` code was added.

**Why:** the error registry is frozen for this ticket and the spec's registry has no in-use code.
`VALIDATION_ERROR` with a typed per-issue list is the closest honest classification: the request is
valid syntax but cannot be satisfied in the current state, and the caller is told exactly which items
block it.

### Field attributes: only `default`, and relaxing `required`

**Choice:** `config set field` accepts `type`, `required`, and `default`, but `type` always fails with
`ACTION_REQUIRES_MIGRATION`; `required true` on an optional field fails the same way; `required false`
succeeds; `default null` clears the default and any other token is parsed against the declared type. An
undeclared key is `VALIDATION_ERROR` (a rename is not a configuration operation — it is a removal plus an
addition, and removal is a migration). `config add field --required` is refused as a migration too.

**Why:** the spec puts type changes, renames, required-field addition, and optional→required in
migration, and relaxing metadata in configuration. Accepting `type` as an argument and then refusing it
gives a caller one consistent place to learn why, instead of an "unknown attribute" error that hides the
migration answer.

### The operator scalar is cleared by an empty value

**Choice:** `config set operator ""` removes the `operator` key (and `--json` still reports
`changed: true`); any other value is trimmed and written.

**Why:** the parser rejects an empty operator when the key is present, so "unset" must be absence. There
is no separate `config unset` in this release.

### Field default and field value text parsing lives with the declared type

**Choice:** `FieldType::parse_value` is the one textual parser: `integer` accepts `-?[0-9]+`,
`boolean` accepts exactly `true`/`false`, `list:*` requires a JSON array checked element-wise, `string`
takes the token verbatim. It mirrors the CLI's custom-field value parsing for the same types.

**Why:** a configuration default and an item field value are the same kind of value against the same
declared type. Keeping the parsing beside `accepts` means a new scalar type cannot be added to one
without the other.

**Open:** the CLI's `field --set` still owns its own equivalent parser for string tokens (JSON string
literals are accepted there). Unifying them is a later cleanup; both agree on the scalar types today.
# T6 — hierarchy, soft links, trees, and the batch engine

### `parent set` replaces, and re-setting the same parent is a true no-op

**Choice:** `set_parent(project, id, parent, options)` always writes the single parent field, so naming
a different parent replaces the old one. When the loaded document already records the requested parent,
`ParentOutcome { changed: false, written: false, fingerprint: None }` comes back and **nothing** is
written — `updated_at` does not move and the bytes stay identical. `clear_parent` behaves the same way
for an item with no parent.

**Why:** the spec says at most one parent, so "set a second" is a replacement, not an error. And a
no-op that still bumps `updated_at` would make a replay of the same command show up as a change in `git
log`, which is exactly what an idempotent agent needs not to happen.

### Parent cycles are rejected at write time, and only diagnosed afterwards

**Choice:** `set_parent` walks the proposed parent chain upward and returns
`CYCLE_DETECTED {kind: "parent", cycle}` with a closed path that starts and ends at the child.
`Graph::parent_cycles()` reports every parent loop the graph can see, so a loop created by an external
edit becomes a finding in `_meta.diagnostics`. Nothing repairs it.

**Why:** a rejected write is cheap to recover from; a loop that reaches disk makes every tree read
ambiguous. The diagnosis after the fact is for edits made outside Waymark, and repairing them silently
would rewrite someone's document on a read, which no read may do. The walk is bounded by the item count
so a pre-existing loop cannot hang the write.

### `parent` is not a dependency and not a lifecycle cascade

**Choice:** a parent edge contributes nothing to readiness, and closing a parent does not close or
reopen a child. The only derived consequences of `parent` are the `children` column in list rows, the
hierarchy tree, and `HAS_ACTIVE_CHILDREN` at archive time (T7).

**Why:** the spec separates structure from scheduling: hierarchy describes what an item belongs to,
dependencies describe what it waits for. Conflating them would make "close the epic" a bulk lifecycle
operation that nobody asked for and that would be very hard to undo.

### Soft links live in `ops::link`, and both documents change in one batch

**Choice:** `add_soft_link` / `remove_soft_link` write `soft_links` on **both** documents, sorted and
deduplicated, in one `write::replace_batch` call: prevalidate both, stage both, replace both, roll back
the first if the second fails. A duplicate link, a self-link, or a cross-kind relationship is
`INVALID_RELATIONSHIP {relation: "soft_links"}`; a missing target is `ITEM_NOT_FOUND` and an archived
one `ITEM_ARCHIVED`.

**Why:** a symmetric edge stored on one side only is a lie that depends on which document you read.
Sorting and deduplicating makes the persisted bytes a function of the set, so two agents that add the
same pair in either order converge instead of producing a spurious write conflict.

### Removing a soft link tolerates a missing or archived partner

**Choice:** `remove_soft_link` requires that the caller's own document lists the partner. If the
partner's document is missing or archived, only the surviving side is rewritten
(`LinkOutcome::other == None`), instead of failing.

**Why:** this is the same asymmetry dependencies use, for the same reason: an archived or deleted
partner cannot be rewritten, and refusing would leave the dangling half-link only repairable by hand.
Archived documents stay untouched because archive is a tombstone whose relationship fields are
historical.

### `write::replace_batch` is the shared all-or-nothing engine, and it does not take the lock

**Choice:** `replace_batch(&[BatchTarget]) -> Result<Vec<BatchApplied>, WaymarkError>` prevalidates
every target (regular file, fingerprint matches, no path twice), stages every new document as a synced
temporary beside its destination, then renames them in order. On a replace failure it restores the
already-replaced documents from bytes captured during prevalidation; if a restore also fails it returns
`ROLLBACK_FAILED {applied, failed, rollback_failed}` and never reports success. The caller holds the
project write lock; the engine deliberately does not, because a batch usually needs to validate against
the project under the same lock.

**Why:** T7's archive has to rewrite several documents (the archived item plus its parent's or
children's views) with the same promise, and a second implementation would drift. Excluding the lock is
what lets one operation do validation, batch, and projection refresh inside a single critical section
instead of three.

### A mid-batch failure rolls back, but a crash between renames is not a transaction

**Choice:** the engine's contract is about *reported* failure. A process crash between two renames can
leave the earlier documents' new bytes on disk; there is no journal and no recovery pass. The spec's
`ROLLBACK_FAILED` requirement is satisfied because no successful outcome can hide a partial write.

**Why:** a real multi-file transaction needs a journal, and the spec puts `doctor` (T14) in charge of
repair rather than asking every write to be crash-atomic. Documenting the boundary keeps a later ticket
from assuming a guarantee this does not give.

### Tree depth: `None` unlimited, `Some(0)` the start alone, `Some(n)` `n` levels below

**Choice:** `hierarchy_tree(project, root: Option<&str>, depth: Option<usize>)` and
`dependency_tree(project, id, depth, flat)` both read depth that way. A scoped hierarchy read starts at
the named root and never walks upward.

**Why:** `--depth 1` means "the children" in the spec's own example, and that only works if the start is
depth zero. `None` for unlimited keeps "no flag" distinct from "zero".

### Trees keep unresolved references as nodes, never as silence

**Choice:** the hierarchy tree has a separate `unresolved: Vec<UnresolvedItem>` section for active items
whose recorded parent is missing, archived, or part of a loop; they are not promoted to roots.
`dependency_tree` renders a missing dependency as a `TreeNode { item: None }` leaf. Both reads attach
the graph's diagnostics to `_meta`, report `complete`, and write nothing.

**Why:** the spec says invalid or missing parent references must not suppress valid trees and unattached
items belong in a clearly explained unresolved area rather than being hidden or treated as ordinary
roots. A `TreeNode` with no item is the one representation that cannot be mistaken for a real item,
because every renderer must handle the `None` case.

### Siblings are ordered by stable ID, and a repeated node is a leaf

**Choice:** children are sorted by stable ID in both trees, matching the list family's final tie-break.
The dependency tree is a depth-first spanning tree: the first time an item is reached it is expanded,
and every later reach is a `repeated: true` leaf. A dependency cycle therefore terminates with a
repeated leaf instead of looping.

**Why:** the graph is a DAG, not a tree, so *some* rule has to pick which occurrence is expanded; stable
ID order makes the choice identical on every machine for the same snapshot. Marking the repeat keeps
the fact that the edge exists visible while bounding the output by item count as well as by depth.

### The flat dependency form is pre-order rows with `depth` and `from`

**Choice:** with `flat`, `DependencyTree::flat` is a pre-order `Vec<DependencyRow>` of
`{id, item, depth, from, repeated}`; `from` names the row's parent row. The nested tree is still built,
because it costs nothing extra and the CLI renders either.

**Why:** a row list is the shape a table renderer wants, and `from` is what lets it draw the edges
without re-deriving them. Keeping both forms out of one walk avoids two traversals that could disagree.

### Tree and relationship proofs are one level deep, and never repair

**Choice:** `hierarchy_tree` and `dependency_tree` are read-only and take no `OpOptions`; they never
take the store lock. `Graph::from_project` still fails the whole read on an unparseable document, which
is the list family's existing behaviour.

**Why:** a read that silently drops a broken sibling would render a tree that looks complete and is not,
which is worse than an explicit `PARSE_ERROR`. Scope-local reads around a broken sibling are the
projection ticket's rule to make.

### The CLI renders hierarchy and dependency edges with different markers

**Choice:** `waymark tree` indents children with `  - `, `waymark dep tree` marks a dependency with
`  -> `. Human output always goes to stderr and JSON to stdout; both carry the shared envelope. The
argument shapes are `parent set <ID> <PARENT_ID>`, `parent clear <ID>`, `link add|remove <ID>
<OTHER_ID>`, `tree [--root ID] [--depth N]`, and `dep tree <ID> [--depth N] [--flat]`.

**Why:** a dependency tree and a hierarchy tree can contain the same items, and a shared marker would
make a pasted terminal transcript ambiguous. One envelope keeps every adapter on the same result shape,
which the spec requires of CLI and MCP alike.

### Archive reads the target without the archived-item write refusal

**Choice:** `ops::mediated` exposes `load_entry_any`, which reads and fingerprint-checks a document but
does not apply the `ITEM_ARCHIVED` refusal; `load_entry` keeps the refusal and is what every other
mediated write uses. `ops::archive` is the only caller of the unguarded form and answers
`ALREADY_ARCHIVED` itself. It also derives the target from the same complete scan it uses for inbound
edges rather than locating it separately once more.

**Why:** the refusal and the re-archive answer are different rejections of the same state, and archive
must produce the specific one. Rather than duplicating the read in archive (and hoping the two reads
agree), the scan is the single source of both the target bytes and its fingerprint. A target this scan
has already read and hashed is also exactly what the batch has to conflict-check against.

### Archive stages its inbound cleanup as one batch, target included

**Choice:** the archived target and every active file it cleans are replaced through a single
`replace_batch` call, target first, then affected files in stable-ID order. A file that loses nothing
is not touched at all — not even a rewrite with identical bytes — so `updated_at` moves only for files
whose content actually changed. The target never gets a new `updated_at`.

**Why:** "all or nothing" has to include the marker, or a failure mid-cleanup would leave an item
archived while an active file still points at it. Excluding the no-op files keeps the batch, and the
conflict surface, proportional to the relationships that actually existed.

### A broken sibling fails archive instead of being skipped

**Choice:** archive scans every document and propagates a parse or ID-mismatch failure. `show` and the
tree family can degrade with a diagnostic because they answer about one item or one subtree; archive
cannot, because the file it could not read might be the one holding an inbound edge.

**Why:** silently skipping the unreadable file would archive the target and leave a dangling active
reference behind, which is exactly the state the inbound cleanup exists to prevent.

### A blank archive reason is stored as no reason at all

**Choice:** `archive_reason` is trimmed, and a whitespace-only value is treated as `None`, so the
document omits the key. The reason is otherwise stored verbatim, including internal newlines, which the
canonical serializer emits as a double-quoted scalar.

**Why:** an empty scalar and an absent key differ in the canonical form but say the same thing to a
human; only one of them can be canonical, so the omitted form wins. Multi-line reasons are real prose
and must not be silently flattened, so they get the serializer's existing quoted form.

### `ALREADY_ARCHIVED` carries `{id}`, exactly as the registry already specifies

**Choice:** the archive ticket's prose says re-archive reports `ALREADY_ARCHIVED {archived_at}`; the
error registry, the spec's code table, and the existing `already_archived` constructor all say `{id}`.
The implementation keeps `{id}` and does not change the registry, the golden file, or the constructor.

**Why:** the details keys are pinned by the golden registry, and `{id}` is the field a caller needs to
correlate the failure — the whole point of the code is that the timestamp is *not* being replaced, so
reporting it would invite a caller to treat it as authoritative. The `ArchiveOutcome` already carries
`archived_at` for successful archives, so no information is lost.

### The archived-only range is exclusive and reports archived rows as closed

**Choice:** `list::Filters` gains `archive: ArchiveRange` (`Active` by default, `Archived` opt-in) with
`with_archived()` / `with_active()` builders; `Graph` keeps archived items in a lookup table exposed by
`archived_items()` and answers `readiness_any` with `Readiness::closed()` for them. `list --archived`
lists retired work; `ready --archived` and `blocked --archived` are empty because their own selection is
readiness, and an archived item has none.

**Why:** the spec forbids mixing archived items into the active graph, so the two ranges cannot be a
single population with a filter predicate. Reusing `closed()` is the only answer that is true of an
archived item and also excludes it from `ready`/`blocked` without a second, parallel rule.

### `show` marks an archived item's own edges as historical

**Choice:** `show` reports `archived` and `relationships.historical: true` for an archived item, and the
human rendering suffixes `parent`, `depends on`, and `soft links` with `(historical)`. The fields
themselves are printed unchanged.

**Why:** the archive operation strips *inbound* edges but deliberately leaves the archived item's own
edges alone as context; without a marker a reader would take them for live references that the graph
has already stopped honouring. The marker is on the relation block and not on each field, so the shape
of the payload does not change.

### `waymark archive` renders the plan on both dry runs and real archives

**Choice:** `waymark archive <ID> [--reason TEXT] [--dry-run]` prints `archived` (or `would be archived`)
followed by the marker instant, the unchanged status, and one line per planned removal naming the
source item and the relation field. JSON carries `removals: [{from, relation}]`, `affected: [{id, path}]`,
`written`, and the full item under `item`.

**Why:** a real archive usually cleans several files the caller never named, so the removals are the
answer the caller needs whether or not they asked for a dry run. `--archived` was added to the shared
`ListArgs`, which puts it on `list`, `ready`, and `blocked` at once — the three already share one filter
vocabulary, and giving only `list` the range would have made `ready --archived` a parse error.

# T8 — collection queries, search, and statistics

### `Filters` grew in place; there is no second filter struct

**Choice:** `query::Filters` is the one filter model. `list`, `ready`, `blocked`, `query`, `search`,
and the cheap `--text` path all consume it, and `ops::list` re-exports it. It gained
`parent_present: Option<bool>`, `has_dependencies: Option<bool>`, `readiness: Option<ReadinessFilter>`,
`fields: Vec<FieldPredicate>`, `text: Option<String>`, `leaves_only: bool`, and
`archive: ArchiveRange` alongside the pre-existing status/category/owner/creator/parent fields.

**Why:** two filter structs would drift, and the acceptance criterion is that one vocabulary serves all
three list commands. Widening in place keeps every existing call site compiling while the new fields
default to "do not restrict".

### `rank` is status group, then dependency *level*, then stable ID

**Choice:** `query::rank_index` computes a level for every active item: all items whose dependencies are
already ranked are released together, and each release is one level. `rank` sorts by
`(status.rank_group(), level, stable_id)`, all ascending. Cycle-trapped items never reach indegree zero
and are appended after the last level in stable-ID order. **The final tie-break is stable ID ascending
in every sort mode, including `--desc`.** A missing custom-field value sorts last in both directions.

**Why:** the spec pins group → topology → ID but not the tie-break, and not whether topology means
"depth" or "position in a walk". A level is the only definition that makes a blocker always sort before
its dependent regardless of creation order while still letting unrelated items interleave
deterministically — a plain depth-first walk would let an early-ID dependent drag its blocker ahead of
everything. Keeping the ID tie-break ascending under `--desc` is what makes a descending page a true
reverse of the ascending one except where values tie, and it means `--limit`/`--offset` pages are stable.

### A `ready`/`blocked` listing still ranks against the whole active graph

**Choice:** `Population::includes_archived` is false for `Ready` and `Blocked`, so neither ever reports an
archived item whatever `--archive` says. But the level is computed over the entire active graph, not just
the reported population, so a ready item whose blocker has just closed keeps its deeper level.

**Why:** the alternative — re-ranking a filtered subgraph — would make an item's position depend on
which filter selected it, so the same item would appear at a different offset on two pages of the same
result set. That breaks the "union of pages equals the full result" property.

### `ArchiveRange` defaults to `active`, and `ready`/`blocked` still respect it for input

**Choice:** `ArchiveRange::{Active, Archived, All}` parses from `active|archived|all`, defaults to
`Active`, and is carried on `Filters` (and therefore on `Query` and in the structured JSON document). An
archived item is never ready and never blocked: it waits on nothing.

**Why:** active-only is the safe default for a human reading a work queue, and an explicit range is the
only way to see the archive. Making `ready`/`blocked` silently widen to `All` would report items nobody
can act on.

### The structured query document is the CLI's escape hatch, and it is plain JSON

**Choice:** `waymark query --filter <JSON>|@file|-` parses a JSON object through `Filters::from_json`:
`status`, `category`, `owner`, `creator`, `parent`, `parent_present`, `dependencies`, `readiness`,
`leaves`, `text`, `archive`, and `field: [{key, op, value}]`. `null` for a filter, an unknown key, an
`exists`/`missing` predicate carrying a value, an undecodable `archive`, and any operand that disagrees
with the declared field type are all `VALIDATION_ERROR`. The document owns the filters; the flags still
own sort, window, and projection.

**Why:** clap flags cannot express every conjunction, and a second CLI grammar would be a second place for
the model to drift. `--where "KEY OP [VALUE]"` stays as the convenience spelling of one predicate on
`list`/`ready`/`blocked`/`search`, and the JSON document is what scripts and the MCP server use.

### Field predicates are typed against the declared schema before anything is read

**Choice:** `FieldOp` is `exists, missing, eq, ne, lt, le, gt, ge, prefix, contains, list_eq, list_ne,
list_contains`. `Filters::validate(&Config)` rejects an undeclared key, `eq`/`ne` on a type mismatch,
ordering on a boolean or a list, `prefix`/`contains` on a non-string, and any `list_*` on a non-list.
`eq`/`ne` on a list compare elements in order; `list_contains` tests membership. Missing never satisfies a
comparison, and `missing`/`exists` ignore the (absent) operand.

**Why:** failing before the scan means a bad filter costs nothing and reports the same code whether the
project is empty or not. It also stops `eq` on an integer field with the string `"3"` from quietly
matching nothing.

### Search reads the body; the cheap `--text` filter does not

**Choice:** `query::Filters::text` matches a case-insensitive substring of the **stable ID and the
title only** — that is what `list --text` does, and it never opens a body. `search::search` matches the
same way against title, every frontmatter scalar (category, owner, creator, status, timestamps, custom
field strings, list elements), and the body. Title hits sort before non-title hits by a stable sort
applied after the shared ordering; the payload is a projected row with `title_hit` added and
`snippets: false` in the data block.

**Why:** the spec separates "cheap list text filter" from "search" precisely because opening every body
is the expensive part; conflating them would make `list --text` as slow as `search`. No snippets keeps
search out of the business of excerpting prose it does not own, and keeps the row shape identical to a
list row.

### Stats is counts only, and says so in the payload

**Choice:** `stats::Statistics` carries `archive`, `completeness`, `freshness`, `total`, `by_status`,
`by_category`, `ready`, `blocked`, an optional one-field `distribution`, and a constant `not_reported`
list of `history, throughput, completion_rate, cycle_time, lead_time, time_series`. `statistics_json`
also emits `counts_only: true`. `--top N --top-field KEY` requires a declared **string** field; a
distribution reports the top `N` values plus an `(unset)` bucket for items without the field.

**Why:** the spec forbids throughput and history but does not say how a caller is meant to know they are
absent rather than empty. Naming them, and naming the archive range and projection completeness in the
payload itself, lets a client tell "no history field" from "no history yet". Counts require a complete
fresh projection, which is why stats never runs against a scoped read.

### Pagination is applied to one snapshot, after filtering, before projection

**Choice:** `Window { limit, offset }` is applied to the fully filtered and sorted vector in one pass.
`limit == 0` means unlimited; `offset` past the end is an empty page that still reports the true `total`.
`total` is always the pre-window count. `DEFAULT_LIMIT` is 100 and `MCP_DEFAULT_LIMIT` is 50.

**Why:** applying the window before the sort, or re-reading between pages, is what makes a page union
skip or repeat rows. One pass over one snapshot is the only construction where the union of pages equals
the full result even while a writer is active.

### The projection vocabulary is closed, and identity columns are never dropped

**Choice:** `FieldName` is a fixed enum of 20 column names (`schema, id, title, category, status, owner,
creator, created_at, updated_at, closed_at, parent, dependencies, soft_links, custom_fields,
archived_at, archive_reason, ready, blocked, blocked_by, children, dependents`), parsed from a
comma-separated `--fields` list; unknown names are `VALIDATION_ERROR`. Frontmatter columns are nested
under `item`, derived columns are top-level, and `id` plus `status` are always emitted even when they are
not in the set. `--compact` is the identity subset. An unset optional value and an empty list are
**omitted**, never `null`.

**Why:** a closed vocabulary is what lets the CLI, MCP, and Web adapters agree on a row shape without a
shared schema file, and it makes a typo an error instead of a silently empty column. Keeping `id` is what
makes a projected row still addressable; keeping `status` is what makes a row renderable as a row.
Omitting unset values matches the canonical document rule "unset optional values are omitted".

## Git history and diff (T11)

### The repository is the only history; nothing is written

**Choice:** `waymark_core::git::{history, diff}` shell out to the `git` binary with
`std::process::Command`; no Git crate is added, no file is written, no lock is taken, and nothing is
recorded under `.waymark/`. There is no audit log, no attribution table, and no second history store.

**Why:** the spec is explicit that Git history and diff are the durable audit trail and that no separate
application audit log or authenticated attribution exists. Shelling out also makes Waymark see exactly
what the user's Git sees (their aliases for objects, their repository config, their worktrees) instead of
reimplementing plumbing through a binding that could disagree.

### Absence is a finding, not a failure

**Choice:** `GitProbe::detect` walks up to a `.git` directory **or** file (a linked worktree's
`gitdir:` pointer is read and resolved) and searches `PATH` for `git`. When either is missing, both
operations return `Ok` with an empty payload and one `NOT_A_GIT_REPOSITORY` finding at severity `info`,
`fixable: false`, on `_meta.diagnostics`. Bad input (an unknown item, a `..` path filter) is still an
error.

**Why:** the registry already declares `NOT_A_GIT_REPOSITORY` with no exit class and finding severity
`info`. A tracker that refuses to list history because a checkout is not a repository would break the
"migration is permitted outside a Git repository" rule for the same reason; and a brand-new repository
with no commits is likewise an empty success, not an error.

### Scope, pathspecs, and rename following

**Choice:** `history` has three scopes: one item's document (`--item`), one explicit project-relative
path (`--path`), or the configured `storage_root`. `diff` has two: one item, or the storage root. Every
pathspec is rewritten against the repository root, so a project that is a subdirectory of its repository
is still scoped correctly. `--item` and `--path` are mutually exclusive. A `--path` is rejected if it is
absolute or contains `..`.

**Why:** the spec says "commit history for work items", and the storage root is the useful default when
the caller does not name one. Pathspec rewriting is what makes the "project lives in a subdirectory"
case — which discovery already supports — behave the same as the common case. Rejecting `..` keeps a
read command from being talked into describing files the project does not own.

**Rename following:** `git log --follow` is used only in the item scope, because Git accepts `--follow`
only with a single path; the storage-root and path scopes are plain `git log`. The report says which
scope produced it, so a caller is never misled about whether a rename was followed.

### Working-tree diffs cover staged and unstaged, and a new item is still a diff

**Choice:** `diff` without `--commit` runs `git diff HEAD -- <pathspec>` (staged plus unstaged). If an
item's document is untracked, it is diffed against `/dev/null` so a just-created item still shows a
whole-file diff; the storage-root scope deliberately reports tracked changes only, exactly as
`git diff HEAD -- <storage_root>` does. `--commit` runs `git show --format= --patch <commit> --
<pathspec>`, so the payload is the patch alone and a merge commit shows what Git shows (usually nothing).

**Why:** "the working-tree diff" in the ticket is ambiguous between `git diff` and `git diff HEAD`;
`HEAD` is the only baseline that includes staged changes, and missing them would make the command lie
about what a commit will contain. The untracked exception is the one place a naive `git diff HEAD` would
return "no changes" for work that plainly exists.

### A commit must not be an option

**Choice:** `--commit` is validated as a revision that is non-empty, does not start with `-`, and
contains no whitespace; anything else is `VALIDATION_ERROR`. Revisions are otherwise passed to Git
verbatim (`HEAD~2`, a branch, a full or abbreviated SHA).

**Why:** `Command` does not use a shell, so this cannot inject a command, but a value starting with `-`
would still be parsed by Git as an option and change which command runs.

### `data` is a stable payload, `_meta` is the contract

**Choice:** `HistoryReport::data_json` emits `scope`, `item`, `path`, `paths`, `limit`,
`repository_root`, and `commits` (each with `id`, `short_id`, `author`, `email`, `committed_at`,
`summary`). `DiffReport::data_json` emits `scope`, `item`, `baseline`, `commit`, `paths`, `empty`,
`patch`, `repository_root`. `_meta.completeness` is `scoped` for an item scope and `complete` for the
storage root. Timestamps use `Timestamp`, the same canonical RFC 3339 rendering as documents.

**Why:** both adapters (and a future MCP tool) need the same payload without a second schema, and reusing
`Timestamp` keeps one timestamp format in the project. Reporting `paths` and `repository_root` lets a
caller reproduce the query by hand with `git`.

### Generated agent documentation is derived from the `clap` tree

**Choice:** `waymark agent-doc` takes the CLI's own `clap::Command` (via `Cli::command()`) and walks it:
command paths, `about` strings, and each argument's flags, value name, requiredness, default, possible
values, and help. `--json` emits the same tree as an envelope; `--command PATH` narrows it to one
subtree. It contains no hand-written prose about behaviour, and `clap`'s synthesized `help` and
`version` arguments are filtered out.

**Why:** the ticket requires documentation that "describes only capabilities that exist and adds no
semantics of its own". Deriving it from the parser is the only construction where a later ticket cannot
forget to update it, and where documentation cannot drift from what the binary accepts. A boolean
switch is rendered without a value placeholder because `clap` otherwise reports `true`/`false` as
possible values, which is parser plumbing rather than a capability.

# T9 — projection, completeness, and cache

### The projection is a second type named `Projection`

**Choice:** `crate::projection::Projection` is the disposable in-memory view of the files. It is not
re-exported at the crate root, where `Projection` remains the pre-existing field projection from
`query`. Callers that need both refer to the new one as `waymark_core::projection::Projection`.

**Why:** `query::Projection` already has that name, is re-exported by `ops::list`, and is part of the
CLI's `projected_row` pipeline. Renaming either would churn a public seam the Web/MCP tickets will
also use; the module path disambiguates without moving anything.

### Freshness is a verification result, not a timestamp

**Choice:** a build walks the inventory twice: the first pass parses (or retains) documents, the second
compares every path, size, and modification time against what the first pass observed. A verified build
is `fresh`; a mismatch retries `LoadOptions::max_attempts` times (default 3) and then returns retryable
`SNAPSHOT_UNSTABLE`. There is no time-based staleness and no background refresh: a held projection is
fresh at the instant it was verified, and `refresh` re-verifies it.

**Why:** the spec names path inventory, size, and modification time as the cheap signals, with the
content fingerprint when needed. Re-reading every document just to prove freshness would double the
read cost for no additional guarantee, so the second pass stays on metadata. The test seam
(`LoadOptions::with_inventory_hook`) lets a controlled writer prove the retry path without racing a
thread.

### Content fingerprints decide re-parsing; metadata only records change

**Choice:** when a refresh sees new size or modification time it reads the bytes. An equal content
fingerprint retains the parsed entry and updates the recorded metadata without parsing; only a changed
fingerprint re-parses and bumps `ProjectionStats::parse_count`.

**Why:** the spec says a metadata-only change with an equal content fingerprint may retain the parsed
entry, and a cache must never be keyed on timestamps. `parse_count` makes that rule assertable without
timing.

### Only unreadable, unparseable, or duplicate *work-item* documents make a projection incomplete

**Choice:** inventory problems for files that are not work items (`.txt`, non-ID `.md` names, symlinks,
uppercase stems) remain inventory problems and do not by themselves mark the projection scoped. An
item-shaped document that cannot be opened, parsed, or whose persisted ID disagrees with its file name
becomes a `ProjectionFailure`, and two documents claiming one ID make both failures.

**Why:** a category directory that contains a stray note is not a partial world, and treating it as one
would make every global read fail for a file Waymark never reads anyway. A duplicate ID, by contrast,
is a real ambiguity: the projection refuses to prefer one file silently, which matches the store's
existing `DUPLICATE_ID` rule.

### A scoped local read stays `direct_file` + `scoped`

**Choice:** `Projection::scoped_read` and `ops::read::read_scoped` return `source: direct_file`,
`completeness: scoped`, `freshness: fresh`, even when the projection behind them is complete.
`readiness` is `None` whenever the projection is incomplete; `children` and `dependents` still come
from the parseable entries, because they are direct observations. Diagnostics carry an
`INCOMPLETE_PROJECTION` finding first, followed by the per-document failures. The CLI `show` command
now uses this path instead of building a second whole-project graph.

**Why:** the item itself was read from its file, and the spec requires every scoped local read to say
so. Withholding readiness is the honest answer when a broken sibling could hide a dependency; the
relationships that are visible in the parsed documents are still useful and are reported.

### The bounded cache holds derived data only and accepts opaque strings

**Choice:** `CacheKey` has exactly three classes — `Body`, `Detail`, `Query` — and no item or graph
variant. Values are `String`s, so an adapter serializes its payload before storing it. `Body`/`Detail`
keys are bound to the item's content fingerprint; `Query` keys are bound to `Projection::stamp()`, a
digest over every entry fingerprint and failure, which a metadata-only change leaves untouched. A stale
key is a miss; `cache_put` ignores a stale key instead of storing it. `reconfigure_cache` clears
evictable data only when the settings actually change, and a disabled cache stores nothing. Parsed
entries and adjacency live outside the LRU, so eviction can never remove a relationship node.

**Why:** the spec permits caching body, detail, or query-result data "for performance only" and
forbids weakening freshness, completeness, ownership, or archive safety. Keeping the cache opaque and
separate from the projection makes that separation structural rather than a rule each caller must
remember.

### Which reads demand completeness

**Choice:** `list`, `ready`, `blocked`, `query`, `search`, `stats`, `tree`, readiness, and `archive` all
go through `Projection::require_complete` (via `Snapshot::load` / `Graph::from_project`) and fail with
`INCOMPLETE_PROJECTION`. Cross-file relationship writes (`dependency add`, `parent set`, link) build
the same graph and therefore fail the same way. Migration does not exist yet; when it lands it should
use the same gate. Single-document reads (`show`, `read::read`) remain scoped.

**Why:** every one of those operations either publishes a whole-project answer or rewrites files it did
not name; a hole in the graph would make the answer or the rewrite untrustworthy. `show` is the only
read in the current command set that is genuinely about one document.

# T12 — read-only Web service (backend)

Phase 1 of T12 is the Rust backend, the development launcher, and the HTTP/security tests. Phase 2 (a
later agent) adds the SvelteKit frontend in `crates/waymark-web/ui/build` and must treat the contract
below as fixed unless it changes both sides and these decisions.

## Dependency set and MSRV result

**Choice:** `axum 0.8`, `tokio 1` (`rt-multi-thread`, `macros`, `net`, `signal`, `sync`),
`pulldown-cmark 0.13` (safe Markdown), `ammonia 4` (sanitizer), plus the workspace crates. Dev-dependency
`tempfile` only; the HTTP tests use a hand-written HTTP/1.1 client over `std::net::TcpStream` instead of
`reqwest` because the tests must send a missing `Host`, a duplicate header, and `Origin: null`, and
because a TLS stack adds no value to a loopback plain-HTTP service.

**MSRV audit:** `cargo metadata --format-version 1` resolved 195 packages. Every package that declares
`rust-version` was compared against the workspace's `rust-version = "1.85"`. The maximum required
version is **1.85**, declared (among others) by `ammonia 4.2.4`, `tokio 1.54.1`, `clap 4.6.7`,
`hyper-util 0.1.21`, `html5ever 0.40.1`, `cssparser 0.38.0`, `hashbrown 0.17.1` (1.85.0), and
`web_atoms 0.3.0`. **No package requires more than 1.85**, so the gate holds and `rust-version` is not
raised. Thirty-six resolved packages declare no `rust-version`; they are unknown, not evidence of a
higher requirement.

**Pin:** `ammonia → url → idna → idna_adapter 1.2.2 → icu 2.x` declared `rust-version = 1.88`. Rather
than raise the workspace MSRV, `crates/waymark-web/Cargo.toml` declares an exact
`idna_adapter = "=1.2.0"`, which resolves the `icu 1.5` line (all ≤ 1.85). This is a deliberate MSRV
pin; removing it or running a wide `cargo update` re-breaks the 1.85 gate, so re-run the audit after
any dependency change.

## Binding, status codes, and failure envelopes

**Choice:** the listener is always `127.0.0.1` (`WebConfig` has only `port` and `assets`; port `0`
asks the OS for a free port). There is no host/interface/LAN option anywhere in the API. Successful
answers reuse `waymark_core::envelope::success_envelope`; domain failures reuse `failure_envelope` and
add a `_meta` block carrying `error.details.diagnostics` (or `[]`). Adapter failures reuse
`adapter_failure_envelope` with the codes `INVALID_QUERY`, `INVALID_ASSET_PATH`, `INVALID_HOST`,
`INVALID_ORIGIN`, `METHOD_NOT_ALLOWED`, `NOT_FOUND`, and `ASSET_NOT_FOUND`.

Status mapping: `INVALID_*`/`INVALID_ID`/`VALIDATION_ERROR` → 400; `ITEM_NOT_FOUND`/`PROJECT_NOT_FOUND`
and unknown routes/assets → 404; every other domain failure (parse, I/O, incomplete projection) → 500
with the registered code, details, and available diagnostics; every non-`GET`/`HEAD` request → 405 with
`Allow: GET, HEAD`. The method check runs **before** Host/Origin validation so "every non-GET/HEAD is
405" has no exception; a rejected cross-origin POST still receives no CORS headers.

## API contract (v1)

`GET /api/health` → `{v, ok: true, data: {status: "ok", service: "waymark-web", version,
read_only: true}, _meta}`. It touches the project not at all.

`GET /api/tree?view=combined|grouped` (default `combined`). `data` for combined:
`{view: "combined", roots: [Node], unresolved: [Unresolved], categories: [Category]}`. For grouped:
`{view: "grouped", groups: [{category: Category, roots: [Node]}], unresolved, categories}`. Only
non-archived items appear; a grouped view lists only categories that have roots, in configured
(key-sorted) order; a cross-category child remains inside its parent's subtree. `Node` is
`{id, title, category: {key, name, shape}, status: "open"|"in_progress"|"closed", state:
"ready"|"in_progress"|"blocked"|"closed", ready: bool, blocked: bool, blocked_reason: string|null,
parent: id|null, dependencies: [id], children: [Node]}`. `state` precedence is closed → blocked →
in_progress → ready (a blocked in-progress item reports `blocked`; the actual `status` is still sent).
`blocked_reason` is prose of the form `waiting on wm-… (unfinished|archived|missing), …` derived from
`Readiness::blocked_by`. `Unresolved` is `{id, title, category, parent, reason:
"parent_missing"|"parent_archived"|"parent_cycle"}`. No synthetic root is invented: every parentless
active item is a root, and an item that no root reaches is in `unresolved`, not promoted.

`Category` is `{key, name, shape}`. `shape` is the category's index in the configured category order
modulo `SHAPES = ["circle", "square", "triangle", "diamond", "hexagon", "star"]`; the index is stable
as long as categories are not inserted/reordered before it. `categories` is included in both tree
views so the frontend does not need a second request.

`GET /api/item/:id` → `{id, title (plain text, never HTML), body_html (sanitized), category, status,
state, ready, blocked, blocked_reason, owner, creator, created_at, updated_at, closed_at, archived,
custom_fields (opaque object), relationships: {parent, children, dependencies, dependents, soft_links}}`.
Custom-field names carry no semantics: values pass through as stored. An archived ID still answers with
`archived: true` and `state: "closed"`; there is no archive browser or list route.

If the graph cannot be built (damaged document, future `INCOMPLETE_PROJECTION`), the request is a
failure envelope with the registered code, its details, and `_meta.diagnostics`; no partial tree is
ever returned as data.

## Safety boundaries

- Host: exactly one `Host` header, `<host>:<port>`, where `host` is `localhost` or an IP for which
  `IpAddr::is_loopback()` holds, and `port` equals the actual listening port. A missing, repeated,
  foreign, or wrong-port Host is 400 `INVALID_HOST`.
- Origin: when present (at most one), it must be exactly `http://<same host>:<same port>` with scheme
  `http`; `Origin: null`, another scheme, another host, or another port is 400 `INVALID_ORIGIN`.
  Requests without Origin proceed after the Host check. No CORS headers are ever emitted.
- Markdown: `pulldown-cmark` rendering followed by `ammonia` with `rel="noopener noreferrer"` on links.
  Raw HTML is sanitized (script/style/iframe/object/embed/svg and event-handler attributes and
  `javascript:` URLs do not survive); titles travel only as JSON strings.
- Assets: `/assets/<path>` maps to `<assets>/<path>`, rejects empty/`.`/`..`/backslash/NUL segments,
  canonicalizes and requires the target to stay inside the canonical asset root, serves regular files
  only, and never falls back to a placeholder. Unknown routes are 404 (no SPA history fallback).
- Read-only: the crate imports only `readiness`, `ops::list`, `ops::tree`, and `ops::read`; no write
  operation or lock module is referenced. The test suite snapshots every project file before and after
  all reads and rejected writes and asserts byte identity.

## Server API and launcher

Public items: `WebConfig { port, assets }` (with `new`/`with_port`/`Default`),
`DEFAULT_PORT = 7331`, `LOOPBACK`, `SHAPES`, `CategoryView`, `WebError`, `Running { addr, url,
shutdown }`, `bind(project, config) -> Running` (for tests and the launcher), `serve(project, config)`
(binds, prints the loopback URL to stderr, waits for Ctrl-C), `ensure_assets(dir)`,
`default_assets_dir()` (`<crate>/ui/build`). `serve` is deliberately not wired into the `waymark` CLI.

`waymark-web-dev`: `--project PATH` (default: current directory, discovered upward), `--port` (default
7331; `0` is accepted), `--assets DIR` (default: the crate's `ui/build`). Project discovery and the
asset check both precede binding; missing or invalid projects and missing `index.html` print a clear
message to stderr and exit 1 without creating, initializing, or modifying anything. The launcher never
runs npm, never downloads anything, and never serves a placeholder. `GET /` without assets returns 503
with preparation guidance (API tests do not need them).

## Core change (additive)

`waymark-core::ops::tree::hierarchy_from_graph(&Graph, Option<usize>) -> HierarchyTree` was added so the
Web read model can build the hierarchy and readiness from one scan. `hierarchy_tree` now delegates its
whole-project path to it (private `with_meta` preserves the existing `_meta`) and the single-root path
is unchanged. No existing public signature or behaviour changed.

## What phase 2 (SvelteKit) must know

- Build the frontend into `crates/waymark-web/ui/build` (the default) or pass `--assets` elsewhere.
- Serve `index.html` at `/` and hashed files at `/assets/...`. With SvelteKit, set
  `kit.paths.assets = '/assets'` (and keep the app base at `/`) so generated URLs match the route.
  The server has no history fallback: use a single entry route at `/` (hash routing or in-app state)
  rather than deep links; unknown paths return 404 JSON.
- Consume exactly the shapes above; `title` must be inserted as text (never `{@html}`), `body_html` is
  already sanitized, `state` drives colour/shape and `blocked_reason` is the text marker, `shape` is an
  index into `SHAPES`, and `categories` is present in both tree views.
- All preferences (selected view, expansion, dependency visibility, scroll) belong in browser storage
  keyed by an identifier derived from the normalized project-root path; nothing is sent to the server
  or written into the project. The API is strictly read-only and has no search/filter/archive/refresh
  endpoint.
- Fetch without an `Origin` header (same-origin requests do this automatically) or with the service's
  exact `http://localhost:<port>` origin. A page opened at `127.0.0.1` gets an origin for `127.0.0.1`,
  which is accepted; do not proxy through another origin or port.

# T12 — read-only Web frontend (phase 2)

Phase 2 adds the SvelteKit single-page frontend in `crates/waymark-web/ui`, its browser-local
preference store, and its tests. It consumes exactly the phase-1 contract above; the only API change is
`project.root` on `GET /api/health`.

## Design direction — "field index"

**Choice:** the overview is a surveyor's field sheet for a work tree: a cool sage paper ground
(`#eef0ea`), instrument ink (`#1a2420`), one deep-teal accent for interaction (`#1e6257`), a light
header band, and no ornament that does not carry meaning. Colour is used only on work state and the
selected row; category identity is the six-shape vocabulary plus the category name, never a category
colour.

- **Type:** one family (the system UI face) for all reading; `ui-monospace` appears only inside literal
  `code`/`pre` from a sanitized Markdown body. The tree title reads at 14px, metadata at 12–13px, and
  the detail title at 16px; there are no display, serif, or tracked-uppercase labels.
- **Hierarchy:** rows are compact (about 34px), indentation is 1.1rem per depth, and a solid vertical
  guide plus an elbow draws the parenthood edge. Work state is a 3px stripe on the row's left edge
  (ready green, in-progress blue, blocked red, closed grey) followed by the state word, with the
  `blocked_reason` prose directly under a blocked row. Custom fields and category values never set a
  colour.
- **Toolbar:** the view control is a quiet segmented radio group; expand/collapse and the global
  dependency toggle are text controls. The blocked count is the screen's one finding, stated in small
  tabular text next to the view control.
- **States:** empty, unresolved, persistence-unavailable, and read-failure each get their own composed
  block; a failure states the code, message, diagnostics, and that no partial tree is shown.
- **No images, fonts, icon package, gradients, or shadows.** The service is loopback-only and offline;
  the only drawing is the inline SVG brand mark, the six category shapes, and the chevron.
- The inspector (`lint.mjs --app`, `inspect.mjs --app`) reports 0 failures at 1440/768/390. Its one
  remaining warning (a large blank area below a short tree) is inherent to a workspace view whose tree
  grows with the project; it is not filled with decoration.

## Asset paths (supersedes the phase-1 `paths.assets` bullet)

**Choice:** keep SvelteKit's default `/_app/...` URLs and set only `paths.relative = false`; leave
`paths.base` and `paths.assets` unset.

- `kit.paths.assets = '/assets'` from the phase-1 note cannot be used: Kit 2.70 rejects a root-relative
  `paths.assets`, and any non-root `paths.base` would move the one entry route off `/`.
- The server maps `GET /_app/<path>` to `<assets>/_app/<path>` (the on-disk layout SvelteKit writes),
  and keeps `GET /assets/<path>` → `<assets>/<path>` for callers that use that prefix. Both prefixes
  serve the same files; `GET /` serves the shell and there is still no history fallback.
- `adapter-static` writes `ui/build/index.html` and `ui/build/_app/...`; `ui/build` stays a local
  artifact (ignored), so a clean `cargo test` never needs it. `cargo test` includes
  `tests/frontend.rs`, which skips with a message when `index.html` is absent and otherwise fetches `/`
  plus every `/_app/...` URL the shell references, asserting status 200 and a content type.

## Browser-local preference key scheme

**Choice:** `localStorage` only, namespaced by the canonical project root reported by
`GET /api/health` (`data.project.root`), one project never reading another's keys.

- **Identifier:** `waymark.web.v1:<hex(cyrb53(normalized root))>`. Normalization trims, maps `\` to `/`,
  collapses repeated `/`, and strips trailing `/` except for the filesystem root. The 53-bit hash is
  deterministic and non-reversible; it only separates projects on one origin and is not a security
  boundary.
- **Keys:** `…:view`, `…:expanded:<view>`, `…:dependencies:<view>`, `…:scroll:<view>` for
  `view ∈ {combined, grouped}`. Preferences are therefore remembered separately per view, and the
  stored scroll offset is a plain integer (0 when unreadable).
- **Defaults:** `combined`, fully expanded, dependency overlays hidden, scroll 0. A stored empty
  expansion array is respected (a collapsed tree stays collapsed).
- **Failure:** storage is optional. If `localStorage` is absent, blocked, or a write throws (quota,
  revoked permission), the session keeps working in memory and a single non-blocking notice reports
  that preferences may reset. Nothing is written into the project and nothing is sent to the service.
  `project.root` is canonicalized on the server when possible, with the opened root as fallback, so two
  spellings of one project share a namespace without any write to the project.

## Reference-node anchoring

**Choice:** in the grouped view a cross-category dependency produces one reference node and dashed
leader per dependent, anchored to the referenced item's parent when it has one and to the referenced
item itself when it is a root. Hiding dependency overlays hides the reference nodes and the dependency
rows together. The node names the referenced title when the anchor is its parent, and in both cases
names the dependent as "referenced by …". `Tree` children remain under their real parent, so a
cross-category child is never promoted to a synthetic root or a group root.

## Frontend dependency and test set

`@sveltejs/kit 2.70.3`, `svelte 5.57.3`, `@sveltejs/adapter-static 3.0.10`, `vite 8.3.5`,
`typescript 6.0.3`, `vitest 5.1.0`, `@testing-library/svelte 5.4.2`, and `jsdom 30.1.3`, pinned by
`package-lock.json`. `npm ci` installs exactly the lockfile; `npm run build` produces the static SPA;
`npm test` runs vitest over `ui/tests`; `npm run check` runs `svelte-check`.

The vitest suites cover combined and grouped rendering (including a cross-category child under its real
parent), dependency overlay toggling and reference anchoring, detail contents and the single
`body_html` injection sink, per-node and whole-tree expansion, browser persistence and per-project
isolation, unavailable/failing storage, empty, unresolved, and read-failure states, and the GET-only
API seam. The Rust suite keeps the HTTP/security coverage and adds the built-asset check above.

## Launcher and developer commands

From the repository:

```sh
cd crates/waymark-web/ui
npm ci
npm run build
npm test
cd ../../..
cargo run -p waymark-web --bin waymark-web-dev -- --project /path/to/project
```

The launcher never runs npm, downloads dependencies, or serves a placeholder; a missing `ui/build`
exits 1 with the install/build guidance. `cargo build` and `cargo test` do not need npm or `ui/build`.
# T13 — version and schema migration

## Readable generations

- `FORMAT_GENERATION = 1` is the generation this release writes; `PREVIOUS_FORMAT_GENERATION =
  0` is the oldest it reads. `model::is_readable_generation` is the one window check, and both
  [`parse::parse_document`] and [`Config::load`] reject anything outside it with
  `UNSUPPORTED_VERSION` (not `PARSE_ERROR`/`CONFIG_INVALID`). A project at generation `0.REV` still
  opens, which is what makes an explicit version migration possible.
- A document's staleness is release-relative: `WorkItem::needs_migration` is `schema <
  FORMAT_GENERATION`; `WorkItem::needs_migration_from(generation)` is the planner's target-relative
  form. `ItemNeedsMigration` details carry the project generation from the configuration (usually
  the same value).
- Ordinary mediated writes preserve a document's persisted generation, so even a generation-0
  project never gets a lazy upgrade. `ITEM_NEEDS_MIGRATION` is raised by
  `mediated::require_current_generation`, used by `load_entry` and by `archive`.
- Read models mark rather than hide: `ListedItem::needs_migration`, the derived
  `FieldName::NeedsMigration` column (`needs_migration` in list/query JSON), and the `show` payload
  (`data.needs_migration` and `data.item.needs_migration`, plus a human `migration needed` line).

## Plan/execute scaffold (version and schema, reused by phase B)

- Public shape: `MigrationPlan { kind, description, targets: Vec<PlannedTarget>, config:
  Option<PlannedConfig>, warnings }`, `MigrationReport { kind, description, dry_run, affected,
  targets: Vec<error::TargetOutcome>, count, written, config_changed, previous_revision, revision,
  meta }`, and `preflight(project) -> Preflight`. `PlannedTarget` owns the exact canonical bytes, so
  execution never re-serializes a plan.
- There is no persisted plan file, plan id, or plan/apply handshake. `plan_version`/`plan_schema`
  are read-only and public; `execute(project, plan, options)` repeats every check (configuration
  readable, complete and fresh projection, one fingerprint comparison per planned target, every
  planned document parsed again, planned configuration parsed again) and only then writes.
  `migrate_version`/`migrate_schema` rebuild the plan inside the project write lock immediately
  before `execute`, so a preview followed by a hand edit re-derives and succeeds correctly; a plan
  object passed straight to `execute` after an edit fails `MIGRATION_PRECONDITION_FAILED`.
- All execution failures other than an unrestorable rollback become `MIGRATION_FAILED` with
  `per_target` outcomes (`applied`/`skipped`/`failed`). The failing path is taken from the error
  location, then `details.path`; when no path can be identified every target is marked `failed`.
  `ROLLBACK_FAILED` from the batch engine is passed through unchanged.
- Configuration and documents are one batch: `write::replace_batch_with_file(&[BatchTarget],
  Option<FileTarget>)` is the writer. A conflict on the configuration carries a null `id` because it
  has no work-item ID; migration translates batch failures before an adapter sees them.
- Dry runs report the same affected set, counts, configuration delta, and revisions, and mark every
  target `skipped`; they never write. (The lock file already exists in an initialised project.)

## Version migration choices

- `--to` accepts only `FORMAT_GENERATION`; every other value is
  `MIGRATION_UNSUPPORTED_GENERATION {requested, supported}`. The user chooses *when* to migrate, not
  which generation to declare.
- The configuration revision increments by exactly one only when the configuration generation
  changes (one configuration content write). A project already at the target generation but holding
  older documents rewrites exactly those documents and leaves the revision alone.
- `updated_at` is preserved. A generation rewrite is not a content change, and the same rule is
  applied to schema migrations so a plan is deterministic and independent of the clock; the
  configuration revision is the record of the migration. Bodies, custom fields, relationships, and
  archive markers (`archived_at`, `archive_reason`) are preserved by construction (only `schema`
  changes).

## Schema migration choices

- Exactly one action per run. `RemoveField(key)` deletes the declaration and the key from every
  projected document, archived ones included. `AddRequiredField { key, ty, default }` adds (or
  promotes from optional, matching the `ACTION_REQUIRES_MIGRATION` recovery command
  `waymark migrate schema --add-required-field`) the declaration and fills the default only where a
  document has no value.
- An existing value is never converted or dropped. A value the declared type cannot hold is
  `VALIDATION_ERROR`; a missing value with no default (request or existing declaration) is
  `VALIDATION_ERROR` naming the items, so no item is left invalid.
- Type changes (including list element type changes) are refused by the migration verbs with
  `ACTION_REQUIRES_MIGRATION` and exactly the `waymark migrate schema` command the configuration
  surface names. Renames have no migration verb; optional→required *is* the
  `--add-required-field` action.
- The configuration revision increments by one exactly when the declaration content changes; an
  idempotent add with an identical declaration changes no configuration content and does not move
  it.
- This release has no whole-project item-vs-configuration validator yet (`validate` covers the
  configuration only), so the "removing a declared field leaves an undeclared key" drift cannot be
  asserted here. The migration removes the key everywhere it can see, including archived documents,
  which is the guarantee this ticket can make; when the whole-project validation lands, a key that
  survives by a hand edit is the `undeclared key` drift the specification describes.

## Outside Git and preconditions

- A migration outside a Git repository proceeds. `preflight` adds
  `Warning::from_error(WaymarkError::not_a_git_repository())` to `_meta.warnings` saying Git-based
  recovery is unavailable. The envelope's warning field is `Warning` (shared registry code), not
  `Finding`.
- `MIGRATION_PRECONDITION_FAILED` carries every diagnostic it has: incomplete/freshness findings
  plus the broken paths, per-file fingerprint disagreements, and configuration issues. Configuration
  load failures (including an unreadable/unsupported generation) are translated here rather than
  surfacing as `CONFIG_INVALID`/`UNSUPPORTED_VERSION`, and every collected `ConfigIssue` is included.

## Phase B (storage migration) contract

Phase B must use `ops::migrate::preflight(project)` for the shared read-only gates (configuration
readability, complete and fresh projection, per-file fingerprint agreement, Git warning) and add its
own path/destination checks on top, returning `MIGRATION_PRECONDITION_FAILED` with every diagnostic
before any write. It must reuse `MigrationPlan`/`MigrationReport`/`TargetOutcome` and the
`written`/`count`/`config_changed`/`_meta` shape, rebuild its plan inside the project lock, repeat
`preflight` immediately before executing, and report per-target outcomes on failure. Its directory
applier runs between those two steps; the generic `replace_batch_with_file` is only for
document/configuration rewrites (a category-key migration's value rewrites can reuse it).

## Test updates that follow from the above

- `tests/document.rs`: an unreadable generation was "a parse error for now"; it is now
  `UNSUPPORTED_VERSION`.
- `tests/config.rs`: a generation outside the readable window is `UNSUPPORTED_VERSION`, the previous
  generation parses, and older generations report the readable window in details.

# T13 — storage migration

## Actions and what each changes

- Exactly one action per run, enforced by a CLI `ArgGroup` and by a domain
  `StorageAction`. `Root { path }` changes `storage_root` and re-bases every category path under it;
  `CategoryPath { key, path }` changes one declaration and moves one directory; `CategoryKey { from,
  to }` changes the key, moves the directory, and rewrites the persisted `category` value on every
  affected work item.
- A root change derives each category's new path by stripping the old storage root and joining the
  new one. A category path that is not under the declared storage root cannot be re-based, so the
  run is refused with a `category_outside_storage_root` diagnostic instead of guessing. A root
  change to the same value is a content no-op: no moves, no revision bump.
- A category-key change is not a pure relocation. The new directory path is the old path's parent
  joined with the new key (so `work/task` becomes `work/chore`, and a custom `lanes/task` becomes
  `lanes/chore`). The new key must not already be declared (`category_exists`). This keeps a
  configured nesting stable while still letting the directory follow the key. The key change
  rewrites `category` through `serialize::to_document`, so archive markers, `updated_at`, bodies,
  custom fields, and relationships are preserved byte-for-byte otherwise.

## Prevalidation

- Planning runs `ops::migrate::preflight` first (configuration readability, complete/fresh
  projection, per-file fingerprint agreement, Git warning) and then its own read-only path checks.
  Every diagnostic is collected; nothing is written.
- `MIGRATION_PATH_CONFLICT` (frozen details `{target, conflict}`) is used for an existing,
  overlapping, or case-colliding target: equality, ancestry, or descent against an existing category
  path (including the source's own ancestry/descent, which a rename cannot perform), a duplicate
  target between two moved categories, an existing file, and an existing non-empty directory. An
  existing **empty** directory is an allowed target because `rename(2)` can replace it.
- Every other rejection is `MIGRATION_PRECONDITION_FAILED` carrying `diagnostics`: absolute paths,
  paths escaping the repository root, wrong separators, empty components, a symbolic link in the
  target path or any component it traverses, a missing or unreadable source directory, a source file
  the directory digest cannot read, a cross-filesystem target, a target parent that is not a
  directory, an unreadable target, a category outside the storage root, a declared target key, and
  every configuration issue in the resulting document.
- A missing category directory is a precondition failure even though the inventory treats it as an
  empty category: a relocation has no source directory to rename. This is stricter than reads by
  design, and it is the one place "source files missing" is interpreted to include the directory.
- The frozen `MIGRATION_PATH_CONFLICT` details shape cannot carry a list, so when a run finds more
  than one path conflict the `conflict` detail joins every conflict (`path: reason; …`). When
  precondition diagnostics accompany conflicts they are appended to that string; a run with no
  conflict reports every diagnostic structurally.
- Case equivalence compares target paths against every source and every surviving category path
  case-insensitively, and additionally scans each existing target component against its directory's
  entries for a case variant. Symbolic links are checked with `symlink_metadata` from the repository
  root through every target component.
- `MIGRATION_PATH_CONFLICT` and `MIGRATION_PRECONDITION_FAILED` are both in the version exit class,
  so the adapter needs no new mapping.

## Execution and rollback

- A pure relocation is one `rename(2)` per category directory. Nothing is copied and no document is
  rewritten; bytes, fingerprints, and `updated_at` are untouched. Missing target parents are created
  during execution and remembered, so a failure can remove them again. A target on another filesystem
  is detected in prevalidation by comparing the device of the source with the nearest existing
  ancestor of the target, and a rename that still returns `EXDEV` is reported as a clear failure; the
  migration never silently falls back to copying.
- A category-key migration applies the renames first, then writes the rewritten documents and the
  configuration as one `replace_batch_with_file` batch. If that batch fails, every applied rename is
  restored in reverse order and the created directories are removed; only then is `MIGRATION_FAILED`
  returned. If a restore or cleanup fails, the run is `ROLLBACK_FAILED` naming the applied, failed,
  and not-restored paths (merging the batch engine's own lists when it also failed its rollback).
- `migrate_storage` rebuilds the plan inside the project lock immediately before execution, so a
  preview followed by a hand edit re-derives correctly. `execute` given a plan directly repeats every
  read-only check: fingerprints of every rewritten document, the configuration fingerprint, every
  move's source-directory digest, and the whole path/destination prevalidation. A change between
  planning and execution is `MIGRATION_PRECONDITION_FAILED`; a stale plan is never applied.
- The plan carries `moves: Vec<PlannedMove>` (from, to, source digest) and `category_change:
  Option<CategoryChange>`; the report carries `moves: Vec<MoveOutcome>` and `rewritten:
  Vec<RewrittenCategory>`. The version/schema plans leave these empty. `count` remains the number of
  rewritten documents, not moved directories. There is still no plan file, plan id, or plan/apply
  handshake.
- Failed execution reports `TargetOutcome`s for directory destinations, rewritten-document
  destinations, and the configuration path; move targets that were restored are `skipped`.

## Configuration and report

- The configuration and the moves agree by construction: the next configuration is derived from the
  same plan that produces the moves and is serialized before any write. The revision increments by
  exactly one when content changes; a no-op does not move it, and a failed run leaves both the
  configuration and the files as they were.
- The storage dry run repeats the full read-only check, reports every move as `skipped` and every
  rewritten value, and writes nothing — not even a lock file write (the lock file already exists in
  an initialised project).
- Outside Git the run proceeds with the `NOT_A_GIT_REPOSITORY` warning that `preflight` attaches to
  `_meta.warnings`.

## What the diagnostics ticket (T14) must know

- No registry code was added or changed. The new diagnostics are details values inside the existing
  `MIGRATION_PRECONDITION_FAILED.diagnostics` array, with `kind` values `absolute_path`,
  `escaping_path`, `invalid_path`, `symlink_in_path`, `source_missing`, `source_unreadable`,
  `cross_device`, `target_parent_not_a_directory`, `target_unreadable`,
  `category_outside_storage_root`, `category_exists`, and `config_issue` (plus the existing
  `incomplete_projection`, `fingerprint_disagreement`, `unreadable_fingerprint`,
  `config_unreadable`, and `projection_unreadable`).
- `MIGRATION_PATH_CONFLICT` still has only `{target, conflict}`; multiple conflicts are folded into
  `conflict`. A structured list would be a registry change and is deliberately not made here.
- `MIGRATION_FAILED.per_target` now legitimately contains directory paths (move destinations) as
  well as document and configuration paths. `ROLLBACK_FAILED` lists absolute directory paths in
  `applied`, `failed`, and `rollback_failed`.
- Report JSON adds `moves` (`{from, to, outcome, error?}`) and `rewritten`
  (`{id, path, from, to}`). `affected`, `targets`, and `count` keep their version/schema meanings.

# T14 — diagnostics and health

## One read-only run, two commands

- `doctor` and `validate` call the same core run: `ops::doctor::{diagnose,
  diagnose_with, diagnose_against}`. `validate` adds no second review; it is the
  compatibility review the specification requires, phrased as project
  validation.
- A full report carries `findings: Vec<Finding>`, counts by severity (`errors`,
  `warnings`, `info`), `strict`, the reviewed `scope`, the permitted `repairs`,
  and `_meta`. The machine data also carries `healthy` and `valid` (true when no
  error findings remain), keeping the previous `validate` JSON shape usable.
  `Finding` is the existing envelope type: there is no second diagnostic
  taxonomy and no new registry code or details key.
- The full scope is `["configuration", "work_items", "graph", "projection",
  "inventory", "lock"]`. A report made without an openable project is honestly
  configuration-scoped (`["configuration"]`) rather than claiming a work-item
  review it could not perform.
- Reported findings never change the exit code by default. `--strict` returns
  the registry's invalid-data class (exit 3) while error-severity findings
  remain. A warning-only report exits 0 in both modes. This deliberately changes
  the previous `validate` behaviour, which failed on configuration findings
  without a flag; the CLI test was updated.
- Strict mode still emits the success envelope: the report is the answer, and
  the exit class is attached to it rather than replacing it with a failure
  envelope. This supersedes the earlier
  `### config validate and validate exit 3 when findings are errors` and
  `### validate covers configuration only, and says so` entries: `config
  validate` still fails on errors, while the project-wide `validate` follows the
  strict contract.

## What is diagnosed, and under which existing code

- Malformed, unreadable, and oversized frontmatter: `PARSE_ERROR` from the
  projection's failures, plus an explicit size check. The size limit is
  provisional: `MAX_FRONTMATTER_BYTES = 64 KiB` — above every real document and
  well below a size that turns YAML parsing into a denial-of-service surface.
- Duplicate IDs and multiple files for one ID: `DUPLICATE_ID` from the
  projection's duplicate rejection. A case variant of a live ID (a filename that
  normalizes to an existing ID) is also `DUPLICATE_ID`; a non-lowercase filename
  with no live counterpart is `PARSE_ERROR`, because the inventory correctly
  refuses to treat it as a document. The old inventory skip was silent.
- ID/path mismatch: `ID_PATH_MISMATCH` from the projection.
- Invalid fields: `VALIDATION_ERROR` split into one finding per issue from the
  projection failure, each with a `field` location. Undeclared custom fields,
  wrong declared types, and missing required fields after a configuration change
  are `VALIDATION_ERROR` with a `custom_fields.<key>` field location. This is
  how schema-removal drift surfaces without touching the document.
- Missing relationship targets: the graph's `DANGLING_REFERENCE` findings.
  Archived targets are also `DANGLING_REFERENCE`: the graph deliberately treats
  archived items as known, so the diagnostic run adds the distinct archived
  case itself. Neither case is repaired.
- Dependency and parent cycles: `CYCLE_DETECTED` from `Graph::diagnostics`. The
  closed path is in the message (`a -> b -> a`); `Finding` has no details field
  and the registry details shape stays frozen.
- Unsupported future file generations: `UNSUPPORTED_VERSION` from the parse
  failure. A future configuration generation cannot produce a `Project` at all,
  so `diagnose_config` reports it (with a `config_key` location) on a
  configuration-scoped report.
- Projection state: an incomplete projection is `INCOMPLETE_PROJECTION` and its
  failures are reported individually. Graph diagnostics (dangling references and
  cycles) are skipped unless the projection is complete and the inventory has no
  uncertain paths: a partial graph is never published as a complete review.
- Uncertain inventory paths (symlinks, non-document names, unreadable entries):
  `INCOMPLETE_PROJECTION`, because a skipped file can hide a node or an edge.
- Fingerprint changes after the projection read the bytes:
  `INCOMPLETE_PROJECTION` naming the path. The run re-reads each parsed document
  once more rather than trusting the earlier scan.
- New, deleted, moved, and changed files: `INCOMPLETE_PROJECTION`, but only
  through `diagnose_against(project, &retained_projection, options)`. A CLI
  invocation builds a fresh projection and has no "before" state; the MCP
  server, which retains a projection, is the intended caller. This is why the
  delta seam is public.
- Path conflicts between category directories: the existing configuration
  validator's `CONFIG_INVALID` findings (equal paths and ancestor/descendant
  overlaps) are part of every report. No second conflict walker was added.
- Timestamp incoherence: `TIMESTAMP_INCOHERENT` (warning). Provisional rules:
  `updated_at < created_at`; `closed_at < created_at`; `archived_at <
  created_at`; `closed_at` present while the status is not `closed`; and status
  `closed` with no `closed_at`. Future timestamps are not diagnosed because the
  specification does not define "future" against a clock for a diagnostic run.
- Items needing migration: `MIGRATION_AVAILABLE` (info), one finding per item
  with a `work_item` location. A configuration generation older than the current
  format generation adds one finding without a location (the registry allows
  only `work_item` for that code).
- The lock probe is `is_store_locked` (one non-blocking `try_write`). When the
  lock file does not exist the probe returns "not held" without creating it, so
  a read-only diagnostic run adds no file. When held it reports one info-level,
  non-fixable `LOCK_HELD` finding naming the lock path and nothing about the
  holder; it never waits.

## Repair boundary

- All findings are `fixable: false`. A diagnostic run may rebuild only the
  disposable in-memory projection and reset the in-process cache; it never writes
  a work-item document, relationship, body, custom field, or configuration
  value. `Report::repairs` reports those two rebuilds with what changed and which
  error findings remain. No repair command or `--fix` flag was added.
- A test snapshots every file before and after a deliberately defect-filled run
  and asserts byte identity; a second test proves a probe on a project without a
  lock file does not create one.

## Registry gaps recorded, not filled

- There is no code for "inventory changed" or "file moved"; the existing
  `INCOMPLETE_PROJECTION` is the only suitable finding code, and its frozen
  details are not used by `Finding` at all. A dedicated code would be a registry
  change and is deliberately not made.
- There is no finding code for a self-dependency or a duplicate relationship
  edge (`INVALID_RELATIONSHIP` is not a finding code). A self-dependency already
  surfaces as a `CYCLE_DETECTED` path; duplicate edges are not separately
  reported.
- `MIGRATION_AVAILABLE` has no `config_key` location kind, so a configuration
  generation drift finding carries no location.
- `Finding` has no details map, so structured cycle paths and inventory deltas
  live in the message; exposing typed details would be an envelope contract
  change (not made here).

# T10 — MCP adapter (phase A)

## Transport and framework

- `rmcp 0.8.5` with its default features plus `transport-io` (`base64`,
  `macros`, `server`, `transport-io`); the client feature is a dev-dependency
  only, for the in-process test harness. `rmcp` is edition 2024 and declares no
  `rust-version`; `cargo metadata --locked` reports no package in the resolved
  220-package graph above `rust-version = 1.85` (the highest declared is
  `tokio 1.54.1` at 1.85, then `schemars 1.2.3` at 1.74, `axum 0.8.9` at 1.80).
  The workspace `rust-version = "1.85"` was not raised.
- The binary is `waymark-mcp`. Transport is stdio only, via `stdio()` and
  `ServerExt::serve`, then `waiting()`. There is no HTTP transport, no listener,
  and no resources/prompts surface — tools only.
- The Tokio runtime is `rt-multi-thread` with `macros`; the server is a
  `#[tool_handler]` implementation whose `tool_router` is assembled from the
  per-family `#[tool_router]` blocks (project, config, read).
- `serverInfo` identifies the server as `waymark-mcp` (title `Waymark`, the crate
  version), not rmcp's build-environment default. One test drives the real
  binary over a piped stdio pair (initialize, tools/list, tools/call, EOF) so
  the process boundary and framing are covered, not only the in-process client.

## Project context

- The start directory is `--project DIR` when given, otherwise the process
  working directory. A non-existent explicit directory is a startup failure
  with the registry's `PROJECT_NOT_FOUND` exit class (5), printed to stderr.
- The project is resolved *per tool call* with `Project::discover`, not cached
  at startup: the server is long-lived, and a configuration or file change must
  be visible to the next call exactly as it is to the next CLI invocation.
- A missing or invalid project does **not** abort startup. `waymark_project_init`
  must be able to create a project where none exists, and
  `waymark_config_validate` must be able to report a broken configuration.
  Startup writes one narrative line to stderr; each tool that needs a project
  returns the registry failure (`PROJECT_NOT_FOUND`, `CONFIG_INVALID`, …) in a
  business envelope. stdout is the protocol channel and is never used for
  narrative.
- `waymark_project_init` uses the start directory as the project root (like
  `waymark init`) and reuses an existing configuration unless `force` is true.

## Envelope and failure representation

- Success: `CallToolResult::structured(success_envelope(data, meta))`; the
  envelope is both the text content and the structured content. `_meta` comes
  from the operation that produced the data (`direct_file` for config/project
  reads, `memory_projection` for collection, tree, search, stats, and
  diagnostics).
- Business failure: `CallToolResult::structured_error(failure_envelope(error))`
  — a tool result with `isError: true`, never a JSON-RPC error. The error object
  is exactly the shared `{v, code, message, details, location?, retryable}`;
  the adapter adds no field and no second classification. `invariants` and the
  registry comparison test pin this.
- JSON-RPC errors are left to `rmcp`: unknown tool and parameter
  deserialization failures are `-32602 invalid params`, and transport failures
  are transport errors. No tool handler constructs one.
- Diagnostic reads (`waymark_config_validate`, `waymark_validate`,
  `waymark_doctor`) return a success envelope containing the findings, `valid`
  (and `healthy`), counts, and scope, even when findings are errors. The CLI
  fails `config validate` with `CONFIG_INVALID` and exits 3; MCP has no exit
  code and the report is the requested capability, so the presentation differs
  while the registry codes, locations, and severities do not. A report that
  cannot be produced at all is still a business failure.

## Arguments and defaults

- Collection tools take a structured filter document (`filter`) parsed by
  `Filters::from_json`, exactly like `waymark query --filter`; the adapter does
  not re-declare the filter vocabulary. The generated schema therefore types
  the document as an arbitrary JSON object and the tool description lists the
  keys — typing it in the adapter would duplicate validation and turn domain
  errors into protocol errors.
- `archive` is a top-level argument on list/ready/blocked/stats and overrides
  `filter.archive` when both are present, so one request always has one range.
- MCP collection default is `limit = 50` (`waymark_core::query::MCP_DEFAULT_LIMIT`)
  for list, ready, blocked, and search; the CLI default stays 100
  (`DEFAULT_LIMIT`). `limit = 0` means unlimited in both adapters, and
  `offset` defaults to 0. The description is the agent-facing documentation of
  the default because JSON Schema cannot express it here.
- `fields` (projection) and `compact` are mutually exclusive. An empty
  `fields` list is a `VALIDATION_ERROR` rather than "everything".
- `waymark_search_work_items` rejects a blank `text` with `VALIDATION_ERROR`
  (the adapter-level check the CLI performs as a usage error); `waymark_core`
  documents an empty needle as matching everything for callers that have
  already validated.
- `waymark_get_stats` uses `top` + `top_field`; `top` without `top_field` is a
  `VALIDATION_ERROR`, and the archive range defaults to `active`.

## Shared-surface invariants and follow-ups

- `waymark_mcp::invariants::batch_parameters` is the public, phase-independent
  check that a tool schema accepts no `items`/`ids`/`targets`/`batch`/`changes`
  parameter and no array of objects. The catalog test applies it to every
  advertised tool; phase B must keep it passing for every write tool.
- `waymark_mcp::PHASE_A_TOOLS` is the phase-A catalog; the catalog test compares
  it with the served tools, so phase B must extend it or the test fails.
- The project-init configuration assembly and the machine JSON renderers
  (item, config, list page, tree node) are adapter-local copies of the CLI's
  representation because `waymark-core` exposes no shared renderer. Only
  representation is duplicated; no rule is. Moving those renderers into core is
  a phase-B follow-up, not a phase-A edit to core.
- Phase B must add: config writes and category/field management; create, edit
  (title/body/custom fields), status, owner, move, parent, dependency, and
  soft-link tools; archive and migration; Git history/diff; generated agent
  documentation; and the CLI/MCP parity test. The diagnostics tools here already
  call `ops::doctor::diagnose`; the write tools must likewise return the shared
  envelope and the shared codes.
# Owner transfer and item move

- **Owner identity validation is shared with create, not restated.** The
  non-empty / single-line / trimmed rule lives in one helper
  (`ops::create::owner_issue`) that both creation and `owner transfer` call, so
  the two surfaces cannot drift. A transfer refuses an invalid identity before
  taking the lock or reading an item. The refusal target is `owner`.
- **The `OWNER_MISMATCH` warning is computed against the owner the item had
  before the transfer.** The operator is the identity initiating the handover, so
  comparing it with the *new* owner would warn the outgoing owner merely for
  transferring an item they legitimately own. A transfer by the current owner
  therefore produces no warning even though the persisted owner changes.
- **An unknown category in `move` reuses creation's `VALIDATION_ERROR` issue**
  (`"<key> is not a category declared in the project configuration"`) through the
  same shared helper (`ops::create::category_issue`). It is not
  `CATEGORY_NOT_FOUND`: creation never used that code for an undeclared category,
  and the brief requires parity with creation.
- **The destination-conflict code is `WRITE_CONFLICT`, not
  `MIGRATION_PATH_CONFLICT`.** `WRITE_CONFLICT` is the registry's conflict class
  (exit 4) for "a target must not be overwritten", its frozen details shape
  (`{id, path, expected_fingerprint, actual_fingerprint}`) fits exactly
  (`expected: "absent"`, `actual: "present"`), and `create` already uses the same
  shape for a path that unexpectedly exists. `MIGRATION_PATH_CONFLICT` is exit
  class 6 with a migration-specific `{target, conflict}` shape and belongs to the
  migration vocabulary; it is not used by an ordinary move.
- **A regular work-item file already sitting at the destination surfaces as
  `DUPLICATE_ID`, not `WRITE_CONFLICT`.** The target path is always
  `<category-dir>/<id>.md`, so such a file necessarily claims the same stable ID
  and the inventory diagnoses the duplicate before any move rule runs. The
  `WRITE_CONFLICT` destination check covers a non-document occupant (for example a
  directory) and a file that appears after the inventory scan; both are still
  refused without overwriting. Tests cover both shapes.
- **A document that already sits at the destination path is rewritten in place.**
  If the computed destination equals the loaded document's path but the persisted
  category differs (a hand-placed file), the move skips relocation and rewrites
  the category through `mediated::commit` (fingerprint-checked, atomic). It is a
  real change (`changed: true`) because the persisted value moves.
- **The fingerprint check for the cross-path write is done immediately before the
  destination write, under the same lock.** The write engine's
  `replace_checked` compares the fingerprint of the path it writes; a move reads
  the old path and writes the new one, so the check is re-expressed there with the
  engine's exact `WRITE_CONFLICT` shape. The destination itself is written with
  the engine's atomic writer, and the old path is removed only after that write
  succeeds. If removal fails, the newly written destination is removed first, so
  the operation never leaves two documents claiming one ID.
- **A move moves `updated_at`.** The persisted `category` value changes, which is
  a content change, consistent with archive's relationship cleanup and ordinary
  editing; it is not a pure relocation (unlike a storage migration). A
  same-category move is a true no-op: no write and no timestamp move.
- **The persisted category is authoritative for the same-category no-op.** A
  document whose bytes declare the requested category is not relocated merely
  because it was hand-placed elsewhere; the same-category request returns
  `changed: false` and writes nothing.
- **Outcome fingerprints are singular (`Option<Fingerprint>`).** Each operation
  writes exactly one document; the fingerprint is the written bytes, `None` on a
  dry run or a no-op. This mirrors `EditOutcome`/`StatusOutcome` rather than
  archive's multi-document `Vec`.
- **`move` is registered as `ops::r#move`.** `move` is a Rust keyword; the module
  file is `ops/move.rs` as requested, exposed as `pub mod r#move;`. Callers use
  `waymark_core::ops::r#move::move_item`.


# T10 — MCP adapter (phase B)

## Catalog and invariant helpers

- **The catalog constant is `waymark_mcp::TOOLS` (39 tools).** Phase A's
  `PHASE_A_TOOLS` remains as the 14-tool phase-A slice, listed literally because
  a const slice cannot be built from a subslice of another const slice on the
  workspace's Rust version. The catalog test asserts that `TOOLS` starts with
  exactly `PHASE_A_TOOLS` and that the served tool list equals `TOOLS`, so the
  two cannot drift and a tool cannot be added without being named.
- **`invariants::batch_parameters` is applied to every advertised tool, not only
  the write family.** It is the schema-based check from phase A (no
  `items`/`ids`/`targets`/`batch`/`changes` parameter and no array of objects);
  every phase-B tool passes it. The write-schema test additionally pins the
  write family's shapes, including that a `clear` list is an array of strings
  rather than an array of target objects.

## Write options and shared semantics

- **Every write constructs `OpOptions::new()` and adds only `dry_run`.** The
  clock is the system clock, the repository's configured operator is the
  effective operator, and the lock wait is the core default. There is no
  per-call operator, clock, or lock-timeout argument: those are process/operator
  concerns, and the CLI's `--lock-timeout-ms` has no MCP equivalent because an
  agent has no useful way to choose one.
- **Business failures are the shared envelope, never JSON-RPC.** Every handler
  is wrapped in `envelope::respond`, so a `WaymarkError` becomes
  `{v, ok:false, error}` with the registry code, typed details, location, and
  retryable flag. JSON-RPC errors remain reserved for malformed parameters,
  unknown tools, and transport failures.
- **`_meta` comes from the domain outcome.** Work-item writes return the
  outcome's meta, which already carries `OWNER_MISMATCH` warnings. Configuration
  writes use `Meta::direct_file_scoped()` and migration reports use their own
  meta (memory projection, complete, with warnings), matching the CLI's JSON
  mode.
- **Response `data` mirrors the CLI's `--json` block for the same family.** This
  is representation only: the fields come from the same outcome structs
  (`id`, `path`, `changed`, `written`, the complete `item`, and family-specific
  fields such as `changed_fields`, `previous_status`, `removals`, or
  `fingerprints`). The adapter-local `item_json`/`item_fields` renderers from
  phase A are reused inside the adapter rather than copied a second time.

## Creation: a recorded core gap

- **`waymark_create_work_item` exposes `title`, `category`, `owner`, `body`,
  and `dry_run`.** The specification's tool table also names `creator`, custom
  fields, `parent`, dependencies, and soft links, but `waymark_core::ops::create`
  and the CLI's `create` command accept none of those. The tool does not compose
  follow-up writes: creation mints identity and a document in one atomic write,
  and attaching fields/edges afterwards would be a non-atomic multi-write the
  domain does not define. This is reported upward as a genuine core gap; the
  adapter does not invent a composite creation rule.
- **A dry-run create still allocates a preview ID and path** through the core's
  dry-run path and reports `written: false`; nothing is written and no ID is
  reserved.

## Custom-field changes

- **`waymark_update_custom_fields` requires a `custom_fields` object with `set`
  and `clear` halves.** `set` is a JSON map of key to value, `clear` is a list
  of keys. There is no editor, no current-value form, and no optional "maybe
  change" shape. The whole map is required; either half may be empty.
- **An explicit `null` in `set` reaches the domain as
  `FieldChange::Null` and is rejected with `VALIDATION_ERROR`.** It never means
  "clear": the domain distinguishes the directions and the adapter preserves
  that distinction rather than collapsing null into absence.
- **Floats and objects are refused adapter-side with `VALIDATION_ERROR`.** The
  core's `FieldValue` has no representation for them, so no domain rule can
  accept or reject them; the refusal reuses the shared code and issue shape
  (`fields.<key>`). String, integer, boolean, and homogeneous scalar lists are
  handed to the domain, which owns the declared-type check.
- **Duplicate keys cannot reach the domain through MCP.** JSON objects collapse
  duplicates before the adapter sees them; the core's sequence-shaped input
  (which exists so the CLI can report duplicates) is unchanged. A key present in
  both `set` and `clear` is still reachable and is rejected by the domain.

## Configuration writes

- **Scalar and attribute values are strings in their CLI literal grammar.**
  `waymark_config_set` takes `name` + `value`, `waymark_config_update_field`
  takes `key` + `attribute` + `value`, and `waymark_config_add_field` takes an
  optional textual `default`. These are the exact tokens the CLI passes; parsing
  stays in `ops::config`, so the adapter does not acquire a second value grammar.
- **`waymark_config_update_category` changes the display name only.** The
  display name is the only category attribute `ops::config` can change in
  place; the other attribute, the directory path, is a storage migration and is
  therefore not an optional second field. Exposing it here and refusing it would
  be an adapter-side rule the domain does not have.
- **A type change or making a field required is `ACTION_REQUIRES_MIGRATION`.**
  Those refusals come from the domain, with its recovery command, not from an
  adapter-side allow-list. A required declaration via `add_field` reaches the
  same refusal. The tools never route such a change to a migration implicitly.
- **Every configuration write supports `dry_run` and returns the effective
  configuration, `changed`, `changed_keys`, and the revision move.** A no-op
  write reports `changed: false` and an empty `changed_keys`.

## Migration

- **Exactly one action per call is structural, not flag reconciliation.** Schema
  migration takes
  `action: {kind: "remove_field" | "add_required_field", ...}` and storage
  migration takes
  `action: {kind: "root" | "category_path" | "category_key", ...}`. The action is
  a `oneOf` union in the generated schema, so supplying two actions is not
  expressible, and a missing or unknown `kind` is a JSON-RPC invalid-params
  failure, mirroring the CLI's required, non-multiple `ArgGroup`.
- **Version migration takes `target_generation`; the domain decides whether it
  is supported.** A generation this release does not write stays
  `MIGRATION_UNSUPPORTED_GENERATION`, not an adapter-level refusal.
- **A dry run uses the same `migrate_*` entry point with `dry_run` set.** It
  performs the complete preflight, fingerprint re-checks, and plan, and the
  report's `dry_run: true`, `written: false`, `affected`, `targets`, `moves`, and
  `rewritten` blocks are the dry-run contents. Nothing is written, including
  directories and the configuration revision.
- **Action `default` values use the declared type's literal grammar** (for
  example `"0"` for an integer), matching the CLI's `--default`; the domain
  parses and validates them.

## Git inspection

- **`item` and `path` are both optional in history and their mutual exclusivity
  is the domain's `VALIDATION_ERROR`.** The adapter does not duplicate that
  rule; it only normalizes a supplied item ID. `limit` defaults to the core's 20
  and `0` means unlimited.
- **Outside a repository both tools are successful reports.** The domain returns
  an empty report with the informational `NOT_A_GIT_REPOSITORY` finding in
  `_meta.diagnostics`; the adapter surfaces that as `isError: false` and
  `ok: true`, exactly as the CLI exits successfully. Turning it into a business
  failure would invent a rule the domain does not have.

## Tests and CLI/MCP parity

- **Equivalence tests drive the real `waymark` binary.** They copy one project,
  run the same write through MCP and the CLI, and compare the whole tree. For
  work-item writes, only operation-clock frontmatter values (`created_at`,
  `updated_at`, `closed_at`, `archived_at`) are normalized, because both
  adapters use system time; every other byte, including canonical field order,
  must match. Configuration and migration writes have no clock-dependent bytes
  and are compared byte-for-byte.
- **The dry-run test warms the write lock before its first snapshot.** The lock
  file is the mutual-exclusion artifact of a mediated write, not business data;
  taking it once first lets the assertion be strict byte identity for all 23
  dry-run-capable tools.
- **`WRITE_CONFLICT` is exercised through the move tool.** A directory at the
  destination is not a work-item document, so the inventory skips it and the
  move reaches its own destination check; the test asserts the shared
  `{id, path, expected_fingerprint, actual_fingerprint}` details.
  `ITEM_NEEDS_MIGRATION` is exercised by hand-downgrading one document's
  `schema` literal, as the core tests do.
- **Parity is bidirectional.** The test parses the top-level CLI commands from
  `waymark --help`, maps every command family to its tools, asserts every
  advertised tool is reachable from some family, and asserts the exception list
  is exact and named in this document. The deliberate exceptions are:
  - `query` — covered by `waymark_list_work_items`'s structured filter
    document; MCP has no separate query tool.
  - `start`, `close`, `reopen` — covered by `waymark_set_status` with
    `in_progress`, `closed`, and `open`; they are CLI conveniences, not separate
    capabilities.
  - `agent-doc` — a generated CLI reference for humans and agents reading the
    command tree, not a business capability, so it has no tool.
- **No `waymark-core`, `waymark-cli`, or `waymark-web` source was changed.** The
  one genuine gap found (creation's richer inputs) is recorded above rather than
  patched around in the adapter.

# Composite creation

- **Creation is one operation, not a sequence of writes.** `CreateRequest` carries
  `title`, `category`, `owner`, `creator`, `body`, `fields`, `parent`,
  `dependencies`, and `soft_links`; the catalog row is fully implemented in
  `waymark_core::ops::create` and both adapters only translate arguments. There is
  no "create then patch" path.
- **Validation order is fixed: request checks, then planned-edge checks, then the
  lock, then the same edge checks again.** Title, category, owner, creator, and
  every custom field are validated together before the lock. Referenced IDs are
  normalized (`INVALID_ID`) before any target lookup. A create with relations then
  loads a complete, fresh projection and validates self-reference, duplicates,
  existence/active state, and cycles against the *planned* ID before the lock, and
  again under the lock with the ID actually minted. A plain create (no relations)
  never requires a projection, so an unrelated broken sibling does not block it.
- **The planned ID is allocated before the lock only when the request has a
  relation.** Cycle detection needs the future ID: an existing document may hold a
  dangling reference to it, and that is the only way a fresh node can close a
  parent or dependency cycle. The ID is reused under the lock when still free; if
  a concurrent create consumed it, a fresh ID is minted and the same checks run
  again. A refused relation create therefore consumes an ID candidate but writes
  nothing.
- **`Graph::path` and `parent::parent_cycle` are shared with the standalone
  operations.** Composite creation does not get a second cycle walker; the
  helpers were made crate-visible for exactly this reuse.
- **`creator` defaults to the effective operator**, the same identity
  `OWNER_MISMATCH` compares against (operation override, else configured
  operator). It is never inferred from the owner. A supplied creator is trimmed
  and validated with the owner rule, under the issue path `creator`.
- **Custom fields are declaration-checked, not default-filled.** Undeclared keys,
  wrong types, duplicate keys, and missing required fields are `VALIDATION_ERROR`
  issues under `custom_fields.<key>`; a declared default is applied only by the
  migration that adds a required field, not by ordinary creation. `FieldValue` has
  no null, so an explicit JSON `null` is refused adapter-side in the shared
  `VALIDATION_ERROR` shape.
- **Relations keep the standalone rules and codes.** A missing target is
  `ITEM_NOT_FOUND` with `relation` set to `parent`, `dependencies`, or
  `soft_links`; an archived target is `ITEM_ARCHIVED` with `operation: "create"`.
  Self-reference and duplicate references are `INVALID_RELATIONSHIP`, a parent
  cycle is `CYCLE_DETECTED {kind: "parent"}`, and a dependency cycle is
  `CYCLE_DETECTED {kind: "dependency"}`. No new registry code was added.
- **The batch writer grew a create target instead of composing existing writes.**
  `waymark_core::write::BatchCreate` + `replace_batch_with_creates` extend the
  all-or-nothing engine: a create target must be absent (otherwise
  `WRITE_CONFLICT` with expected `absent`), it is staged and renamed with the same
  discipline, and rollback removes a created document instead of restoring bytes.
  `BatchTarget`'s public shape did not change, so archive and migration callers are
  untouched.
- **Parent and dependency edges write only the new document.** A soft link is
  symmetric, so every link partner is loaded for writing under the lock (archived
  or changed partners refuse the whole create), gains the new ID in sorted
  position, and is stamped with the same operation instant. The new document and
  all partners go through one `replace_batch_with_creates` call.
- **The result reports partners and fingerprints.** `Created` carries the item,
  path, `partners` (the rewritten items with their own paths), `fingerprint` for
  the new document, and `fingerprints` in `[new, partners…]` order. A dry run
  returns the same shape with no fingerprints and `written: false`, and takes no
  lock.
- **CLI surface:** `--creator NAME`, `--body`/`--body-file` (mutually exclusive,
  `-` means stdin), repeatable `--field KEY=VALUE` using the same text grammar as
  `field update --set` (the parser is shared), `--parent ID`, repeatable
  `--dependency ID`, repeatable `--link ID`, plus `--json` and `--dry-run`.
- **MCP surface:** `waymark_create_work_item` adds `creator`, `custom_fields`
  (a flat key→value map), `parent`, `dependencies`, and `soft_links`, and returns
  `partners` and `fingerprints` in `data`, matching the CLI envelope. Null and
  non-scalar values are refused adapter-side. The earlier "recorded core gap"
  section above is now closed by this work.

# Review fixes — core operations

## Configuration write fingerprints

- **The expected fingerprint is optional and lives on `OpOptions`.** Configuration
  writes already take the shared operation options, so `OpOptions::with_expected_config_fingerprint`
  adds the conflict token without a parallel options type or a breaking signature.
  Existing callers pass nothing and keep the historical rebase-onto-disk
  behaviour.
- **The fingerprint covers the exact on-disk bytes, not the canonical rendering.**
  The writer reads the file once inside the lock, fingerprints those raw bytes,
  then parses them. A caller that read a hand-edited (but still valid) document
  is not told it changed just because serialization would normalize formatting.
- **A mismatch is `WRITE_CONFLICT` before anything is validated or written.** The
  details mirror the work-item shape (`path`, `expected_fingerprint`,
  `actual_fingerprint`); a configuration document has no stable item ID, so the
  `id` key is absent. The location is `config_key` at the configuration file.

## Lock-free previews

- **Dry runs perform the same checks without the write lock.** Archive and the
  three migration commands branch on `dry_run` before acquiring
  `.waymark/write.lock`, then run the identical planning and validation path. A
  real run keeps the lock for the whole read-modify-write cycle; a preview can
  run while another writer holds it and still reports every planned change. The
  tests hold the lock and run each preview with an immediate lock wait, so a
  regression surfaces as `LOCK_TIMEOUT` rather than a hang.

## Schema migration cannot tighten requiredness

- **`migrate schema --add-required-field` only adds a new declaration.** If the
  key is already declared, the run is refused with `ACTION_REQUIRES_MIGRATION`;
  an optional-to-required promotion uses the exact message and
  `required_command` that `config set field <key> required true` already
  produces. A type change keeps its existing type-change refusal. A genuinely
  new required field with a default is the only accepted shape.

## Category directories and the config critical section

- **`config add category` creates the directory inside the config lock, before
  replacing the configuration.** If the replacement fails, every directory this
  call created is removed again (deepest first, `remove_dir` only, so a
  concurrent writer's files are never deleted). A directory that already exists
  is not touched and does not need rollback.

## Malformed titles are findings, not parse failures

- **The parser still accepts an empty or multi-line `title`.** Rejecting it at
  parse time would make the document unreadable, which the spec does not allow;
  instead the diagnostic run reports `VALIDATION_ERROR` at `title`. `create` and
  `title set` continue to refuse such values before writing.

## Composite-create previews apply the write checks

- **Both halves load soft-link partners through `mediated::load_for_write`.** The
  preview used to call `Project::load`, which skips the archived and
  generation checks; a `--dry-run` could therefore promise a creation the real
  write would refuse with `ITEM_ARCHIVED` or `ITEM_NEEDS_MIGRATION`. The two
  paths now share the mediated read (the real one still re-reads under the lock
  to fingerprint the exact bytes it will replace).

## Naming: `touch`, `commit`

- **`mediated::touch` is now `mediated::stamp_updated_at`.** The spec reserves
  "touch" for a Waymark-controlled frontmatter query, mutation, or archive, and
  the function writes a timestamp. `ops::move` is outside this change's file
  ownership, so it still calls a hidden `touch` compatibility shim; every other
  call site and the implementation use the honest name.
- **`create.rs::commit` is now `write_new_item`.** The old name collided with
  the spec's document "commit" vocabulary; the new one names the whole write
  half (inventory, ID, graph checks, one create-and-partner batch).

# Review fixes — core infrastructure

Provisional decisions made while fixing the core review findings (branch `waymark/fixcore2`). They
record the behaviour the fixes pin; a later agent may adopt `view` in the adapters without changing
these rules.

## One atomic staging path, one destination check

**Choice:** `write::stage_into(path, bytes, prefix)` is the only temp-create/write/flush/fsync/mode
body. The batch engine calls it through `stage` (prefix `.waymark-batch`) and `write_atomic` calls it
directly (prefix `.waymark-tmp`); both then rename through the shared `persist`. `destination_state`
is the only existence/type check: it returns `Absent` or `File` and refuses a symlink or non-regular
file with the same `WRITE_CONFLICT`/I/O shapes as before. `validate_batch_target` maps `Absent` to
the batch's `expected: "missing"` conflict; `write_atomic` accepts both states.

**Why:** the two paths had already drifted in maintenance, and only the symlink/regular-file refusal
separated "safe to replace" from "safe to create". Behaviour, temporary-file names, and error shapes
are unchanged; the existing batch, rollback, mode, and symlink tests are the safety net.

## Item-shaped inventory uncertainty makes the projection scoped

**Choice:** `Projection` is `complete` only when every document parsed **and** no inventory problem
concerns a work-item-shaped path. A path concerns a document when its file stem parses as a
`WorkItemId` (`ops::doctor`'s existing distinction) and it is not a symbolic link. A problem on such
a path makes the projection `scoped`, appears in `diagnostics()` naming the path (`DUPLICATE_ID` for
a case variant of a live ID, `PARSE_ERROR` for a non-lowercase stem with no live counterpart,
otherwise `INCOMPLETE_PROJECTION`), and is listed by `incomplete_paths()`.

Non-item noise stays out: a stray `README.md`, a malformed name like `wm-short.md`, and a `.txt`
note never made the projection partial, because Waymark would never read them. A symlink is
excluded even with a stable-ID stem: the store deliberately does not follow links, so the problem is
the refusal itself and a link in place of a document remains a local `ITEM_NOT_FOUND`, not a global
incompleteness.

**Why:** an uppercase/case-variant document claims an ID the projection may already hold, and a
global read over a world that has not accounted for every claim is not complete. `list`, `tree`,
`stats`, readiness, archive, migrations, and cross-file relationship writes all go through
`require_complete`, so they now refuse instead of publishing a partial graph.

**Supersedes:** `### Only unreadable, unparseable, or duplicate *work-item* documents make a
projection incomplete`, which said uppercase stems remain inventory problems and do not by
themselves mark the projection scoped.

## The structured filter document has one spelling per concept

**Choice:** `Filters::from_json` accepts the documented keys `archive`, `category`, `creator`,
`dependencies`, `field`, `leaves`, `owner`, `parent`, `parent_present`, `readiness`, `status`, and
`text` (`FILTER_KEYS`). `readiness` takes the string values `ready` and `blocked`. The boolean
`ready`/`blocked` keys remain accepted as deprecated synonyms (`DEPRECATED_FILTER_KEYS`) because the
earlier MCP tool description documented them; the `fields` alias for `field` is dropped (it was never
documented), and `leaves` remains the only leaf spelling. The unknown-key `VALIDATION_ERROR` lists
the accepted set and the deprecated aliases.

**Why:** the implementation decisions already pinned `readiness`, and accepting only the boolean
aliases made the documented key a usage error. Keeping the alias documented (rather than deleting
it outright) avoids breaking the adapter surface that advertised it; dropping the undocumented
`fields` spelling removes a second way to say the same thing.

## Reserved field names derive from the query column vocabulary

**Choice:** `model::RESERVED_FIELD_NAMES` is computed at compile time from `query::FieldName::ALL`
minus `NeedsMigration`, in the established order. The value is byte-identical to the previous
21-name list, and a unit test pins it. `FieldName::as_str` is a `const fn` so the derivation stays a
`const`.

**Why:** the list was a third copy of the frontmatter and query vocabularies, so adding a persisted
or computed name required three edits. `needs_migration` stays excluded because it is a derived read
column, not a name a custom field can shadow; the test makes that exclusion deliberate.

## Move outcomes are stated by two constructors

**Choice:** `MoveOutcome::unchanged` (same category: `changed: false`, `written: false`,
`fingerprint: None`, path unchanged) and `MoveOutcome::changed` (`changed: true`, `written:
fingerprint.is_some()`) are the only places the eight fields are assembled. `move_item` passes the
in-place commit's `Option<Fingerprint>`, the dry run's `None`, or the relocation's `Some(fingerprint)`
to the same constructor.

**Why:** the four copies had already drifted (`written: false` vs `written: fingerprint.is_some()`).
Deriving `written` structurally makes the invariant impossible to miss, and an integration test
drives a no-op, a dry run, a committed relocation, and an in-place rewrite asserting
`written == fingerprint.is_some()` for each.

## Dead public API removed

**Choice:** `WorkItem::is_from_the_future` and `lock::with_project_lock_at` are deleted. Neither had
a caller in `src` or in any test (checked with a repository-wide grep); `needs_migration` and
`with_project_lock` are the live spellings.

## One shared JSON renderer module

**Choice:** `waymark_core::view` owns the dependency-free machine payloads the CLI and MCP adapters
publish: `item_fields`, `item_json`, `tree_node_json`, `dependency_row_json`, `row_json`,
`read_json`, `config_json`, `listing_summary_json`, `listing_json`, `write_result_json`, and
`write_result_json_with`. The shapes are copied from the adapters, not redesigned; the CLI tests
remain the contract. The module only reads domain types and returns `serde_json::Value` — no file
access, lock, or policy. The adapters are not edited here; a later agent adopts the module and
deletes its private copies.

**Why:** the two adapters carried byte-identical renderers for the item, tree, dependency row,
configuration, and show payloads, so a payload change had to be made twice and could drift. Keeping
the renderers in core also keeps "what the machine form is" out of the transport crates. `listing_json`
takes the `ArchiveRange` and `Projection` explicitly because those are caller choices, not facts the
`Listing` carries.

# Review fixes — Web frontend

- **One failure block, parameterised by where it appears.** `ReadFailure` is the
  only read-failure presentation, used by both the tree read and the item read.
  Its heading level (`2` or `3`), test id, title, and closing note are props; the
  detail panel supplies `level={3}`, `detail-failure`, "Could not load this item",
  and a note about the panel rather than the tree. Opaque custom-field values are
  rendered with `api.ts::diagnosticText`, the same helper as diagnostics.
- **Escape is scoped by focus, not by a second copy of the panel.** The detail
  panel keeps a window `keydown` listener (it must work from any control inside
  the panel) but ignores the key unless `document.activeElement` is inside the
  panel. A key pressed in the tree closes nothing; the tree keeps its own keys.
  Focus moves into the panel's `<aside tabindex="-1">` on open, and the Explorer
  restores it to the triggering row button on close (falling back to the tree
  pane, which is `tabindex="-1"`, if the row is gone). If a read replaces the
  panel content and drops focus on the body, the panel reclaims it; focus the
  user has deliberately moved elsewhere is never taken back.
- **Browser preferences are keyed by a `ProjectId`, not by a string that may be a
  root or a key.** `projectIdFor(root)` is the single producer, called once in
  the Explorer after `GET /api/health`; `preferenceKey` and `PreferenceStore`
  accept only that branded type. The old prefix sniffing is gone, so a raw root
  can never be mistaken for a namespace.
- **The combined view is a section, not a fake category.** `tree.ts::sectionsFor`
  returns `{kind: 'combined', roots}` or `{kind: 'category', category, roots}`;
  the combined section has no category and no key (so there is no `'*'` sentinel
  to collide with a real category). `visibleRoots`, `summarize`, and `countNodes`
  are pure helpers in `tree.ts`, and Explorer calls them; the persistence notice
  is raised by one `persist()` wrapper around every preference write.
- **The read-failure block groups diagnostics by the codes the projection sent.**
  `failure.ts::readDiagnostics` sorts a failure's diagnostics into the causes a
  reader can act on, using `Finding.code` and `Location.kind` — never a pattern
  matched against message text. `INCOMPLETE_PROJECTION` *with* a location is the
  inventory's own uncertainty finding (`projection::inventory_finding`), so it is
  the uncertain-path group; the same code *without* a location is the
  count-bearing summary of the whole projection, which names no document and is
  therefore set aside rather than listed as a cause. Every other located finding
  is a document problem, including the `DUPLICATE_ID` and `PARSE_ERROR` the
  inventory reuses for a case-variant path: a path that claims a live ID, or is
  not the lowercase form of its own, still names a document to repair. A finding
  located at a config key or a relationship keeps its own group instead of being
  folded into either document cause, and a diagnostic that is not a finding at
  all (a bare string, or an object without a string `code` and `message`) is
  rendered verbatim by the existing `diagnosticText`, so no version skew can make
  the block throw or silently drop what the service said. The guidance line
  ("This view is read-only… Run `waymark validate`") is gated on the failure
  actually being a blocked projection, so a transport failure such as
  `PROJECT_NOT_FOUND` never claims that project documents need repair. Groups are
  lists of findings, not of documents: one document can contribute several field
  findings, and the block states no document count it cannot know. No envelope
  field, endpoint, or write path was added to support any of this.

# Review fixes — adapters

- **A1 — renderers live in `waymark_core::view`, and adapters only add fields
  their operation owns.** The CLI and MCP render the same item, tree node,
  dependency row, config document, listing page, and scoped read through the
  shared functions. Mediated writes that already published exactly
  `{id, path, changed, written, item}` use `write_result_json_with` with their
  extra fields. Two call-site shapes stay deliberately different: `create`
  publishes no `changed` flag (a create is always a write), and `archive` has
  no `changed` and carries a path-less `item` plus `removals`/`affected`, so
  forcing either through the shared core would change the envelope contract.
- **A2 — the string grammar moved into core, not into a second copy.** The CLI
  copy was the more careful one (a JSON string literal is unquoted; a JSON
  array/object is refused for a string field), so that rule now lives in
  `FieldType::parse_value`. `create --field`, `field update --set`, and
  `migrate schema --default` call it; no adapter has a grammar of its own.
- **A3 — `query` takes option (a): the filter flags are gone.** The structured
  document is the only filter input; only the ordering, window, and projection
  flags remain (shared as `ViewArgs`). A filter flag is a usage error rather
  than a silently ignored input, and the rendered `archive` value comes from
  the query that actually ran. This is the truthful end state without inventing
  precedence rules between two filter sources.
- **A4 — `lock_timeout_ms` is a per-operation option on every locking tool.**
  Both adapters translate it through `waymark_core::ops::lock_wait_from_millis`
  (`None` = the ten-second default, `0` = fail fast). `waymark_project_init` is
  the one writer without it: creation cannot contend for a project lock it has
  not created yet.
- **A5 — search is a list-shaped read, not a reduced surface.** The MCP search
  tool takes the same `filter` and `archive` inputs as the list tools; the
  needle selects within that population. The CLI and MCP default limits remain
  deliberately different (100 vs 50), as documented by the `list` tool.
- **A6 — `init` configuration is built by `ops::config::InitRequest` /
  `ops::config::init`.** The domain owns the defaults, the category display
  name, and the path under the storage root; CLI and MCP only translate their
  inputs into the request.
- **A7 — one project-root derivation and one broken-configuration mapping.**
  `ops::doctor::open_for_diagnosis` locates the configuration, derives the root,
  opens the project, or returns a configuration-scoped report for
  `CONFIG_INVALID`/`UNSUPPORTED_VERSION`. `diagnose_from` runs the complete
  review; both adapters call it and no longer carry the block.
- **A8 — diagnostics use the single CLI dispatch.** `run` returns the chosen
  exit for every command; the special doctor/validate match and the dead arm
  are gone. The existing exit contract is unchanged: findings exit 0 by
  default, `--strict` returns the invalid-data class.

# Review fixes — shared display state

- **The display state and blocked-reason prose live in `waymark_core::view`.**
  `DisplayState` (`Ready`, `InProgress`, `Blocked`, `Closed`; `as_str`,
  `parse`, `Display`), `display_state(&Readiness, &WorkItem)`, and
  `blocked_reason(&Graph, &Readiness)` are the one derivation. Precedence is
  `closed` → `blocked` → `in_progress` → `ready` (closed includes archived), and
  the prose is `waiting on wm-… (unfinished|archived|missing), …`. The Web calls
  both directly. The CLI calls `display_state` and keeps only a label mapping
  onto its older column vocabulary (`archived` for an archived item, `ready`
  for an unblocked in-progress item, `-` for a closed `list` row), so its human
  output stays byte-identical; that is presentation, not a second precedence
  rule. `blocked_by` remains the CLI/JSON fact, and the Web derives
  `blocked_reason` from the same helper rather than owning the prose.


# By-category filtering

- **The category scope is stored per project, not per view, and an absent value
  is not an empty selection.** Two tree shapes (`grouped`, and the dependency
  graph a later ticket adds) filter by the same "which categories am I looking
  at" question, so it lives at `waymark.web.v1:<hash>:categories` and is a JSON
  array of category keys. A missing key means "every configured category", which
  is the default a reader meets; `[]` means the reader cleared them all, which
  must survive a reload. Collapsing the two (for example, storing the
  complement, or treating an empty array as unset) would make "clear all" undo
  itself on reload.
- **Stale keys are ignored where the scope meets the payload, not when it is
  read.** `resolveCategoryScope` takes the payload's `categories` and drops keys
  the project no longer configures; `PreferenceStore.readCategories` stays a
  faithful read of what was written. That keeps the resolution testable without
  a browser and lets a project whose categories were renamed be re-added without
  a migration.
- **The control is built from the payload's `categories`, never its `groups`.**
  `groups` only carries categories that have roots, so building the control from
  it would hide a configured-but-quiet category and make it unselectable. The
  cost is that selecting such a category shows no group, which is why the
  grouped view has its own "nothing in the selected categories" state distinct
  from the project-wide "no active work items" state.
- **Filtering the grouped tree never promotes or splices nodes; it keeps the
  ancestors it needs as marked context.** A work item whose ancestors are all in
  scope is kept exactly where it is. A work item in scope whose parent's
  category is out of scope keeps that parent, and every further unselected level
  above it, so the item never becomes a root: parentage is preserved rather than
  invented. Filtering is applied to each group's subtree in place, so a context
  ancestor stays inside the group its real root belongs to and no group is
  re-homed.
- **A context-only group keeps its category's heading, even though the filter
  control shows that category as off.** The group is where the real root lives,
  so the heading tells the truth about the item's category; the context badge on
  the row is what explains why it is on screen while its category is
  deselected. Re-homing the chain under the selected category's heading, or
  inventing a "context" group, would both misreport the category of the items
  shown.
- **A context ancestor contributes only the chain down to selected work.** An
  out-of-scope node is kept if and only if its filtered subtree still holds at
  least one kept node; otherwise it is dropped. That single rule yields both
  halves of the behavior: the whole unselected chain above a selected item
  survives, and an out-of-scope sibling of that chain — a child of a kept
  context ancestor that holds no selected work — does not appear. In-scope
  descendants of a kept context ancestor are ordinary selected work and keep
  their true depth.
- **Context rows are presentation only.** `filterGroups` returns their ids
  separately in `FilteredGroups.contextIds` instead of marking the nodes, so the
  payload shape the server sends is untouched and a row is marked at render time
  by asking whether its id is in that set. They are excluded from
  `FilteredGroups.counts`, so the summary counts selected work only and a blocked
  context ancestor does not inflate the blocked count. They are not
  unresolved-relations material either: that area stays reserved for genuine
  parent problems in the payload, and hiding a kept ancestor there instead would
  turn a presentation decision into an error report.
- **A context row is marked in words, in its accessible name, and in its
  treatment.** The row carries a visible `Context — outside the selection`
  badge, a hatched ground and a dashed state stripe, and the badge text also
  enters the title button's and the toggle's accessible names. Colour alone
  would fail both a screen reader and a grey-scale reading of the screen, and a
  subtle shade would not be "unmistakable at a glance", so the three markings
  are redundant on purpose. The badge itself is `aria-hidden` only because the
  accessible name already carries the same words; nothing is conveyed solely by
  the badge element.
- **The combined view is never filtered or marked.** It is one hierarchy across
  all categories, so it has no category scope and therefore no context rows; the
  Explorer derives `contextIds` from the grouped scope only.
- **The filter lives in `ui/src/lib/tree.ts`, not in the component.** The scope
  resolution, the group filtering and the count wording are pure functions with
  their own unit tests, so the graph ticket reuses the same semantics rather
  than a second copy in another component.

# Web graph view — rendering library

## Svelte Flow, with dagre for the top-to-bottom hierarchy

**Choice:** `@xyflow/svelte` **1.7.1** renders the read-only work graph, and
`@dagrejs/dagre` **3.1.3** computes the fixed top-to-bottom layered layout. Both are pinned to an
exact version (no `^`, no `~`) in `crates/waymark-web/ui/package.json` and in
`crates/waymark-web/ui/package-lock.json`, so `npm ci` resolves them from the lockfile alone. They sit
in `devDependencies` beside every other UI input (the SPA is bundled at build time and served as
static files, so this project has no runtime `dependencies` list). Only
`ui/src/lib/components/GraphView.svelte` imports them (see the integration seam below); nothing else
under `ui/src` may.

**Why:** judged only against this project's contract:

| Contract point | Svelte Flow 1.7.1 | Cytoscape.js 3.34.4 |
| --- | --- | --- |
| Strictly read-only | No write surface at all; nodes are inert unless the app attaches handlers; `nodesDraggable={false}`, `nodesConnectable={false}`, `deleteKey={null}` remove the mutating gestures | No write surface either; both are rendering libraries |
| Fixed top-to-bottom layout | Positions are plain `{x, y}` on each node, so the app supplies them from a layered engine; dagre in `rankdir: 'TB'` produced `a(145,20) b(230,110) c(60,110)` for a 3-node hierarchy | Same model (`dagre` layout preset gives `a(26,0.5) b(51.5,51.5) c(0.5,51.5)`), but positions are only reachable through library objects |
| Solid parent / dashed directed dependency edges | Edge `style: 'stroke-dasharray: 5 5'` plus `markerEnd: {type: 'arrowclosed'}` rendered a dashed path with an arrowhead in the jsdom spike | Same via `line-style: dashed` plus a `target-arrow-shape` on a style selector (both present in 3.34.4) |
| Non-draggable nodes | Per-node `draggable: false` and the global `nodesDraggable={false}` | `autoungrabify: true` and node `grabbable: false` (both present in 3.34.4) |
| App-controlled node content | The node body is app Svelte markup: the existing `Shape.svelte`, category name, state colour, blocked marker and `blocked_reason` render unchanged, and the accessible name comes from the app's own `<button aria-label>` | Node labels are a text field on a canvas; app-styled content means HTML overlays positioned over the canvas, i.e. a second rendering stack |
| Accessibility surface | Each node is a DOM element (`role="group"`, `aria-roledescription="node"`, `aria-describedby` pointing at the library description, `tabindex="0"` when focusable); each edge is `<g role="group" aria-label="Edge from a to b">`; the viewport is `role="application"` and `ariaLabelConfig` localizes the library strings. App controls inside a node are ordinary focusable DOM | Every node is painted pixels; a node has no element, no role and no accessible name. Accessible node semantics would require a parallel hidden DOM tree, which is the "second rendering stack" this ticket forbids |
| Testability from the existing suite | Rendered 3 nodes and 2 edges as real DOM in jsdom; a `<button>` inside the node content was found by role/name, `fireEvent.click` fired the app callback, and the selected state was observable | Rendered mode aborts in jsdom: `Error: Could not create canvas of type 2d`. Headless mode computes positions but produces no DOM, so every test would have to assert library objects |
| Bundle weight | A minimal SvelteFlow page built to 208.16 kB JS (69.69 kB gzip) + 16.32 kB CSS (2.75 kB gzip), against a 25.26 kB (10.14 kB gzip) plain-Svelte baseline: about 60 kB gzip of JS marginal. Dagre adds 48.8 kB minified (17.1 kB gzip) | 490 kB minified (157.9 kB gzip) bundled through esbuild with the dagre extension; cytoscape's own `esm.min` is 434 kB (136 kB gzip) |
| Install footprint | 20 packages, ~3.5 MB unpacked (`@xyflow/svelte`, `@xyflow/system 0.0.84`, `@svelte-put/shortcut 4.2.0`, `d3-drag/zoom/selection/interpolate` and their `@types`) | 2 packages, ~6.4 MB unpacked (`cytoscape` 5.6 MB, `cytoscape-dagre` 810 kB — the extension bundles its own dagre) |
| Maintenance | 1.7.1 published 2026-11-02, MIT, xyflow team; `@xyflow/svelte` ~368 k weekly downloads, `@xyflow/system` ~20 M | 3.34.4 published 2026-10-15, MIT, cytoscape team; ~25 M weekly downloads |

Svelte Flow wins on the two points that decide this ticket: it renders DOM the existing Vitest +
Testing Library + jsdom suite can drive, and its node content is app-owned markup, so the
accessibility contract (semantic roles, accessible names, keyboard operation) is satisfied by the
same components the tree already uses rather than by a parallel representation.

**Rejected — Cytoscape.js 3.34.4 with cytoscape-dagre 4.0.1.** The rejecting evidence is the jsdom
spike, not documentation: mounting a 3-node hierarchy in a real jsdom container threw
`Error: Could not create canvas of type 2d` from `cytoscape.esm.mjs` (`HTMLCanvasElement.getContext`
is not implemented without the native `canvas` package), and the container was left holding an empty
`<div>` with no node elements. The same graph in `headless: true` mode did compute dagre positions, so
Cytoscape is usable only as a model — tests would then assert `cy.nodes()`/`cy.edges()`, which is
exactly the library-internals coupling the specification forbids, or the suite would need a native
canvas dependency and still could not name a node. Canvas painting is the root cause of both
failures: no DOM means no accessible node names and no way for Testing Library to reach a node.

**Open / what would change it.**
- If the suite stopped running in jsdom (a real browser runner with a canvas implementation), the
  testability objection to Cytoscape would weaken — the accessibility objection would not.
- If Svelte Flow made node content canvas-painted, or dropped its `role`/`aria-label` output, the
  decision needs revisiting.
- If the graph bundle budget dropped below ~60 kB gzip (for example a hard performance budget for
  large projects), a lighter library would be reconsidered.
- If the licence or the visible attribution link were unacceptable (see below), a library without one
  would be reconsidered.

**Layout helper, and why dagre rather than elk.** `@dagrejs/dagre` 3.1.3 is MIT, synchronous, works
in jsdom and in Node, and adds one transitive dependency (`@dagrejs/graphlib` 4.0.6). `elkjs` 0.12.1
is `EPL-2.0 OR GPL-3.0-or-later` — a copyleft option in a project whose whole dependency surface is
currently permissive — ships an 8.2 MB package with a web-worker runtime, and its layered algorithm
is heavier than a fixed top-to-bottom rank layout needs. Cytoscape's equivalent helper,
`cytoscape-dagre`, bundles its own copy of dagre and is only reachable through the canvas renderer.

## The integration seam

- `ui/src/lib/graph.ts` owns the **pure** graph model: nodes built from the tree payload (category
  shape, category name, state, blocked marker, accessible name parts), solid parent-child edges,
  optional dashed dependency edges, the visible/collapsed/filtered sets, and the structured text
  alternative's data. It imports no graph library (dagre included) and no Svelte component, and is
  unit-tested directly.
- `ui/src/lib/components/GraphView.svelte` — plus at most small child components under the same
  layer, such as a node component — is the **only** module that imports `@xyflow/svelte` and
  `@dagrejs/dagre`. It receives the model and `onSelect`/`onToggle` callbacks, owns the layout call
  and the pan/zoom/fit/reset controls, and never becomes application state.
- `ui/src/lib/components/GraphText.svelte` renders the structured text/list alternative as plain DOM.
- Tests drive rendered controls and observable state (roles, accessible names, `data-*` hooks the app
  owns, callback effects), never library objects — no `cy` instances, no `nodeLookup`, no
  `useSvelteFlow` store assertions.

## What the graph tickets must know (measured, not assumed)

- **No vitest config change is needed once the package is a declared dependency.** The existing
  plugin chain compiles the library's `.svelte` files as-is. A spike that installed the package with
  `npm install --no-save` did fail with `TypeError: Unknown file extension ".svelte"`; after
  declaring it in `package.json` the same mount passed with the unchanged `vitest.config.ts`
  (`test.server.deps.inline` was tried and is not required).
- **Two jsdom shims are required** before any `SvelteFlow` mount: `window.matchMedia` (Svelte's
  `MediaQuery`, used by the library's initial store, throws `window.matchMedia is not a function`)
  and `ResizeObserver` (`ReferenceError: ResizeObserver is not defined`). Both belong in
  `tests/setup.ts`, not in individual tests. Verified by removing them one at a time from the spike
  setup.
- **Edges need explicit geometry.** jsdom reports `getBoundingClientRect()` as all zeros, so
  `<Handle>` components inside a custom node measure 0×0 and the library renders **zero** edges.
  Giving each node an explicit `width`, `height` and static handle metadata
  (`handles: [{type: 'source', position: Position.Bottom, x, y, width: 1, height: 1}, …]`) produced
  the expected edge paths, including the dashed style and the arrowhead marker — and needs no
  `<Handle>` component at all, which also keeps the mobile/empty state honest. Prefer asserting edge
  semantics from the model; if the DOM must be asserted, keep the static handles.
- **Nodes are addressable.** A node rendered as
  `<div class="svelte-flow__node" role="group" aria-roledescription="node" tabindex="0"
  aria-describedby="svelte-flow__node-desc-1">`; edges as
  `<g class="svelte-flow__edge" role="group" aria-label="Edge from a to b">`; the viewport as
  `role="application"`. Put the app's `<button>` (or link) inside the node content: that is the
  accessible name and the click target, and it drove `onSelect` from a jsdom click. `nodesFocusable`
  defaults to `true` (a second tab stop per node); set it deliberately and record which surface owns
  keyboard selection.
- **Non-draggable** is `nodesDraggable={false}` globally plus `draggable: false` per node;
  `nodesConnectable={false}` and `deleteKey={null}` remove the other mutating gestures.
- **Attribution.** `SvelteFlow` renders a `svelte-flow__attribution` panel containing an outbound
  link to `svelteflow.dev` unless `proOptions={{ hideAttribution: true }}` is passed. The MIT licence
  permits hiding it; the ticket that builds the view must choose and record the choice, because a
  visible third-party link is a product-facing element this ticket deliberately does not ship.
- **Weight, for later budgets.** A minimal graph page is ~60 kB gzip of JavaScript over the current
  shell (69.7 kB gzip total against 10.1 kB gzip today) plus 2.8 kB gzip of CSS; dagre is a further
  ~17 kB gzip. Install is 20 packages, ~3.5 MB unpacked.

## The rendering seam as built

- `ui/src/lib/graph.ts` — the pure model. `buildGraph(data: TreeData): WorkGraph` walks each section
  root in payload order, de-duplicating by id, and returns `nodes` (id, title, category, shape,
  status, state, blocked, blocked reason, parent id, depth, has-children), solid `edges`
  (`{id, from, to}`), `roots`, and `unresolved`. There is no synthetic root: `roots` are exactly the
  parentless items, in payload order. `graphNodeLabel` and `edgeLabel` supply the accessible names
  (`"<title> — <category>, <state>"`, `"Parent-child: <a> → <b>"`). No library, no Svelte, no I/O.
- `ui/src/lib/components/GraphItemNode.svelte` — the library's node slot. It unpacks the node's
  `GraphNodePayload` (`node`, `graphY`, `selected`, `onSelect`) so `GraphNode.svelte` stays a plain
  component that takes work-item values and library props never leak into it.
- `ui/src/lib/components/GraphNode.svelte` — one card: the category's `<Shape>` plus its name, the
  lifecycle status, the title as the single interactive element, the state word with its existing
  `state-*` class, and the blocked marker with the server's reason. It carries
  `data-node-id`/`data-depth`/`data-parent-id`/`data-graph-y` on the card element, so placement and
  single-appearance are assertable without touching the library.
- `ui/src/lib/components/GraphView.svelte` — the only importer of `@xyflow/svelte` and
  `@dagrejs/dagre`. It lays the model out with dagre (`rankdir: 'TB'`, `nodesep: 28`, `ranksep: 56`,
  16 px margins) and hands the renderer fixed geometry. It owns the legend panel; it does not own the
  presentation choice.
- `ui/src/lib/components/Explorer.svelte` — owns the presentation. A second `.segment` radio group
  (`name="presentation"`, labels `Tree` and `Graph`, sr-only legend `Presentation`) switches between
  `<TreeView>` and `<GraphView>`; both render the tree payload already in memory, so switching reads
  nothing. `PresentationName` is stored per project (`<projectId>:presentation`), independently of the
  data view's key, and an unreadable value falls back to `'tree'`.

**Attribution: hidden, deliberately.** `proOptions={{ hideAttribution: true }}`. The Web adapter is
strictly local, read-only and offline; the attribution panel is a visible outbound third-party link,
which is a product-facing element this slice does not ship. The MIT licence permits hiding it. The
library's development build logs a one-off `console.warn` about this; the warning is the notice, not a
defect. This reverses if the project takes a Svelte Flow Pro subscription or decides to credit the
upstream project in the product surface — then remove the option and revisit the visual weight of the
panel in the layout.

**Keyboard selection is owned by the app's button, not the library.** `nodesFocusable={false}`,
`edgesFocusable={false}` and `elementsSelectable={false}`, plus `selectable: false` and
`focusable: false` per node, leave exactly one tab stop inside each card: the app-owned title
`<button>`, which is also the card's accessible click target. The library does not put its own tab
stop on the node wrapper, and no library selection state reaches the app. Edges are non-focusable and
carry an `aria-label` written by the app, so a screen reader reads the relation where it is
encountered instead of a second interactive surface.

**Static handles and explicit sizes.** Each node is given `width`/`height` (216×84) and static handle
metadata (`source` at the bottom edge, `target` at the top edge, both 1×1). jsdom reports zero-size
rects, so a `<Handle>` component measures 0×0 and the library then draws no edges at all; fixed
geometry makes the drawing deterministic and lets the suite assert real edge elements. There is no
`<Handle>` in the markup, and the layout never depends on measurement.
