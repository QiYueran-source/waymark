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
