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

**Why:** the registry is frozen by a later ticket and does not yet contain `UNSUPPORTED_VERSION`.
Introducing the code here would pre-empt that ticket's taxonomy. The reason is stated in the message
so a caller is not sent hunting for a typo.

**Open:** T13 replaces this with `UNSUPPORTED_VERSION` and teaches this release to read the previous
generation.

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
