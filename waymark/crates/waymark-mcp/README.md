# waymark-mcp

Model Context Protocol server for [Waymark](https://github.com/QiYueran-source/waymark),
a local-first work-item tracker that keeps project work in Markdown files inside
your Git repository. An agent talks MCP over stdio and reads and writes the same
project the `waymark` command-line tool uses: there is no daemon, no database, and
no network access.

- MCP Registry name: `mcp-name: io.github.QiYueran-source/waymark`

## Install

```sh
cargo install waymark-mcp
```

The release archives on the
[releases page](https://github.com/QiYueran-source/waymark/releases) also carry a
prebuilt `waymark-mcp` binary for Linux, macOS, and Windows, so no Rust toolchain
is required.

## Run

```sh
waymark-mcp                       # discover the project from the working directory
waymark-mcp --project /path/to/project
```

`waymark-mcp` walks up from its start directory to the nearest
`.waymark/config.toml`. `--project DIR` starts that search at `DIR`; `DIR` may be
a subdirectory of the project. Diagnostics go to stderr, so stdout stays a clean
JSON-RPC channel.

## Client configuration

Claude Code:

```sh
claude mcp add --transport stdio --scope project waymark -- \
  /path/to/waymark-mcp --project /path/to/project
```

Any client that takes the conventional `mcpServers` map:

```json
{
  "mcpServers": {
    "waymark": {
      "type": "stdio",
      "command": "/path/to/waymark-mcp",
      "args": ["--project", "/path/to/project"]
    }
  }
}
```

Configuration for Claude Code, DSH, and a generic JSON client, plus
troubleshooting, is in
[docs/mcp-clients.md](https://github.com/QiYueran-source/waymark/blob/master/docs/mcp-clients.md).

## Tools

Read:

- `waymark_project_info`, `waymark_get_work_item`, `waymark_list_work_items`
- `waymark_search_work_items`, `waymark_get_stats`
- `waymark_get_tree`, `waymark_get_dependency_tree`
- `waymark_get_ready_items`, `waymark_get_blocked_items`

Write:

- `waymark_create_work_item`, `waymark_set_title`, `waymark_replace_body`
- `waymark_update_custom_fields`, `waymark_set_status`
- `waymark_set_parent`, `waymark_clear_parent`
- `waymark_add_dependency`, `waymark_remove_dependency`
- `waymark_add_soft_link`, `waymark_remove_soft_link`
- `waymark_transfer_owner`, `waymark_move_work_item`, `waymark_archive_work_item`

Project and configuration:

- `waymark_project_init`, `waymark_config_read`, `waymark_config_validate`
- `waymark_config_set`, `waymark_config_add_category`
- `waymark_config_update_category`, `waymark_config_remove_category`
- `waymark_config_add_field`, `waymark_config_update_field`

Diagnostics, migration, and history:

- `waymark_validate`, `waymark_doctor`
- `waymark_migrate_version`, `waymark_migrate_schema`, `waymark_migrate_storage`
- `waymark_git_history`, `waymark_git_diff`

Every successful call returns the shared versioned envelope
`{v, ok: true, data, _meta}`. Every business failure returns
`{v, ok: false, error}` with a stable code, typed details, an optional location,
and a `retryable` flag, so a client can tell a malformed argument from a missing
project or a concurrent edit.

## Related crates

| Crate | Contents |
| --- | --- |
| `waymark-core` | every domain rule, shared by the CLI, this server, and the Web view |
| `waymark-cli` | the `waymark` command-line tool |
| `waymark-web` | the read-only Web service over the same project |

## Licence

MIT OR Apache-2.0.
