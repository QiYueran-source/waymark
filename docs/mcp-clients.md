# Connecting MCP clients to Waymark

`waymark-mcp` is the Model Context Protocol (MCP) server for a Waymark project.
It ships in both the basic and Web archives and speaks MCP over **stdio**:
JSON-RPC on stdin and stdout, with diagnostics on stderr. There is no HTTP
transport and no authentication to configure.

Its tools cover the same operations as the `waymark` command — project and
configuration inspection, work-item reads and writes, searches, trees,
statistics, relationships, and Git history — so an agent can work with the same
project the CLI uses.

## Before you configure a client

Run the binary once to check that it is executable and usable:

```sh
/path/to/waymark-mcp --help
/path/to/waymark-mcp --version
```

The examples below use `/path/to/waymark-mcp` and `/path/to/project`. Replace
both with absolute paths. A relative path may resolve against a working
directory the client chooses, not the one you expect.

### Project discovery

`waymark-mcp` discovers the project the same way the CLI does:

- By default it starts at the current working directory and walks up to the
  nearest `.waymark/config.toml`.
- `--project DIR` starts that search at `DIR` instead. It may name a
  subdirectory of the project, not only the project root.

Omit `--project` when the client starts the server inside the project. Pass it
when the client's working directory is unrelated, for example a desktop client
that launches servers from `/`.

Waymark projects are local and the server reads and writes only the project
directory; no network access is needed or attempted.

## Claude Code

Add the server with the Claude Code CLI. The default transport is stdio; the
example states it explicitly:

```sh
claude mcp add --transport stdio --scope project waymark -- \
  /path/to/waymark-mcp --project /path/to/project
```

`--scope project` writes the definition to `.mcp.json` in the current
repository so it can be shared with collaborators; use `--scope user` for all
projects or omit the flag for the default local scope. List configured servers
with:

```sh
claude mcp list
```

Or add the same definition to `.mcp.json` by hand:

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

Claude Code asks you to approve a project-scoped server before it starts it.
After approval, the Waymark tools appear alongside the built-in tools.

## DSH

DSH configures MCP servers as plugin entries. Add a `@deepseek-ai/dsh-mcp-client`
entry to the home patch layer, `$DSH_HOME/cordis.patch.yml` (normally
`~/.dsh/cordis.patch.yml`), or to the active profile's `cordis.patch.yml`
(for example `~/.dsh/profiles/web/cordis.patch.yml`). The file is a YAML array
of patch entries; insert one client entry:

```yaml
- insert:
    - id: mcp-waymark
      name: '@deepseek-ai/dsh-mcp-client'
      config:
        serverName: waymark
        transport: stdio
        command: /path/to/waymark-mcp
        args: ['--project', '/path/to/project']
```

If the file does not exist, create it with that array as its contents. Restart
DSH, or let its configuration watcher reload, after editing the patch.

`serverName` is the namespace for the tools the model sees. With the example
above they are named `mcp__waymark__<tool>`, for example
`mcp__waymark__waymark_list_work_items`; the name must be 1–32 characters of
letters, digits, underscores, or hyphens and unique within the profile. DSH
writes the server's stderr diagnostics to its logs, so a line such as
`waymark-mcp: project /path/to/project` confirms discovery succeeded.

## Generic JSON client

Most MCP clients accept the conventional `mcpServers` map. Use this block as
the client's server configuration:

```json
{
  "mcpServers": {
    "waymark": {
      "command": "/path/to/waymark-mcp",
      "args": ["--project", "/path/to/project"]
    }
  }
}
```

Some clients require an explicit transport field; add `"type": "stdio"` when
they do. Do not configure a URL or an HTTP transport for Waymark — the server
has no HTTP listener.

If the client supports a working-directory setting, you can set it to the
project and omit `--project`:

```json
{
  "mcpServers": {
    "waymark": {
      "type": "stdio",
      "command": "/path/to/waymark-mcp",
      "cwd": "/path/to/project"
    }
  }
}
```

## Verify the connection

1. Start the client and open its MCP server or tool list. The server name is
   `waymark` in all examples above.
2. Run a read-only operation first, such as listing work items.
3. If the tool list is empty, check the client's server log. A startup line
   `waymark-mcp: project <root>` means the project was found. Without it, the
   server did not find a project at or above its start directory.

A missing project does not stop the server from starting: it can still create a
project through its project-initialisation tool, and other tools report the
missing project. Run `waymark init` in the project directory, or correct
`--project`, and reconnect.

## Troubleshooting

- **Gatekeeper on macOS** — `waymark-mcp` is unsigned and not notarized like
  the `waymark` binary. If the client reports that the executable cannot be
  launched, allow it in **System Settings → Privacy & Security → Open Anyway**,
  or see the Gatekeeper section of the repository README. Only do this after
  checking the archive's SHA-256.
- **Nothing on stdout** — the client must read stdout as the protocol channel.
  Do not wrap the command in a shell script that prints banners, prompts, or
  aliases to stdout; send any logging to stderr.
- **Wrong project** — use an absolute `--project` path and remember that it is
  a discovery start directory. A project is the directory containing
  `.waymark/config.toml`.
- **Permission denied** — check that the extracted binary is executable
  (`chmod +x /path/to/waymark-mcp`). The release archives preserve the
  executable bit for normal extraction.
