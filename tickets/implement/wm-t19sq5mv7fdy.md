---
schema: 1
id: wm-t19sq5mv7fdy
title: Make the test suite pass on Windows and promote the CI leg
category: implement
status: closed
owner: frank
creator: frank
created_at: 2026-11-05T13:54:58Z
updated_at: 2026-11-05T15:01:07Z
closed_at: 2026-11-05T15:01:07Z
parent: wm-vfbqmw1r7x0j
---
## Parent

[Waymark Windows platform support](wm-vfbqmw1r7x0j)

## What to build

The observational `windows-latest` CI leg added in #5 now compiles the workspace and runs the whole suite there. It still fails, and every remaining failure is a real Windows behaviour difference rather than a test-harness accident. Make the suite pass on Windows, then promote the leg from observational to blocking.

This ticket exists so the local-user-configuration change can merge on its green legs without pretending Windows is verified. Nothing here blocks that merge.

### Findings the leg reports

Run: [CI #42037261658](https://github.com/QiYueran-source/waymark/actions/runs/42037261658) on commit `575c5e3`. Repro: `gh pr checks 5`, then `gh run view <run> --job <windows-job> --log-failed`.

| Group | Test | Where it fails | Symptom |
| --- | --- | --- | --- |
| Storage migration | `a_category_key_dry_run_reports_the_rewritten_values_without_writing` | `crates/waymark-core/tests/storage_migrate.rs:150` | the "nothing was written" snapshot differs after a dry run |
| Storage migration | `a_category_key_change_rewrites_every_persisted_value_including_archived_items` | `crates/waymark-core/tests/storage_migrate.rs:235` | the persisted snapshot differs from the expectation |
| Storage migration | `a_storage_key_migration_rewrites_the_category_value` | `crates/waymark-cli/tests/migrate.rs:351` | the command exits 6 (version/migration class) where the test expects 0 |
| Storage migration | `migrate_storage_moves_one_category_and_rewrites_the_key` | `crates/waymark-mcp/tests/migrate.rs:293` | the MCP result diverges from the plan |
| Move | `move_work_item_relocates_the_document` | `crates/waymark-mcp/tests/write.rs:506` | the moved document's reported path/state diverges |
| Git history | `history_reports_the_commits_that_touched_one_item` | `crates/waymark-core/tests/git.rs:123` | the reported paths or commit set differs |
| Git diff | `diff_of_a_commit_shows_what_that_commit_changed_for_the_item` | `crates/waymark-core/tests/git.rs:287` | the patch does not match the expected change |
| Git diff | `a_brand_new_untracked_item_still_has_a_working_tree_diff` | `crates/waymark-core/tests/git.rs:304` | an untracked document produces no usable patch; the functional specification already says Windows needs its own handling here |
| Adapter parity | `migrations_match_the_cli` | `crates/waymark-mcp/tests/equivalence.rs:653` | MCP and CLI results differ on Windows |
| Adapter parity | `read_renderings_match_the_cli` | `crates/waymark-mcp/tests/equivalence.rs:718` | the two renderings differ on Windows |

### Root causes to investigate, in this order

1. **Configured paths versus filesystem paths in migrations.** The migration planner compares `moved.from`/`moved.to` against category paths that come from the configuration (`work/task`). Since `config::resolve_relative` now resolves those per component, that comparison may mix a resolved path with a configured string, which would be right on Linux and wrong on Windows. The CLI migration failing with the version exit class suggests a precondition is being judged differently, not a rename failing.
2. **Repository-relative paths that reach Git.** `history`/`diff` pathspecs and patch paths must be Git paths (`/`) on every platform. `Repository::relative`, `project_prefix`, and `repo_paths` were normalized in #5; check what still leaks a `\` — the committed-diff and history failures suggest a second path source.
3. **Untracked patches on Windows.** The functional specification states that a working-tree diff for an untracked item must return a nonempty, applicable new-file patch on every platform and that Windows replaces a platform-specific empty result. Treat this as product behaviour, not a test fix.
4. **Equivalence snapshots.** The MCP parity helper copies a project and compares snapshots; decide whether the comparison should normalize separators or whether the produced payload really differs.

### Already fixed in #5 (do not redo)

- `libc::ELOOP`/`EXDEV` moved behind `cfg`; `is_cross_device` per platform.
- Unix-only fixtures guarded (`store`, `config_ops`, `archive`, `config`, `write`, `projection` case variants, `serve_web`).
- `waymark_binary()` looks for `waymark.exe`; `.gitattributes` pins LF so golden fixtures are byte-stable.
- Configured paths resolve per component; test path expectations are built from components.

## Acceptance criteria

- [ ] Every test in the table above passes on `windows-latest`, or is skipped through the versioned allowlist the functional specification requires, with a stable identifier and reason.
- [ ] No test that covers locking, replacement, rollback, migration, or path validation is skipped.
- [ ] The Windows leg is no longer `experimental`: `continue-on-error` is removed and a red Windows run blocks a pull request.
- [ ] The `cargo test` steps keep `--no-fail-fast`, so one red platform still reports every failure in a single run.
- [ ] The functional specification's Windows statements and this repository's behaviour agree, or the disagreement is written down in `waymark/docs/implementation-decisions.md`.

**Status:** ready-for-agent
