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
