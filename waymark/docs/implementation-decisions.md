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
