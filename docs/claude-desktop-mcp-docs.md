# Claude Desktop Model Context Protocol (MCP)

> Source: https://modelcontextprotocol.io/docs/2026-07-28/develop/connect-local-servers
> Checked: 2026-10-08
> Applicable version: Current official documentation; version-specific requirements are noted below.
> Additional sources: https://modelcontextprotocol.io/docs/2026-07-28/develop/connect-remote-servers, https://github.com/modelcontextprotocol/modelcontextprotocol/discussions/1940
>
> Note: This document is an external documentation mirror/reference. Copyright belongs to the original site; content may be outdated, please refer to the official links. Follow the original site's license when citing or redistributing.

Claude Desktop connects to MCP servers configured in a local JSON file. Servers expose tools that Claude can use with your approval for each action.

## Local configuration (stdio only)

Open **Claude menu → Settings → Developer → Edit Config**. This creates or opens the configuration file:

- **macOS**: `~/Library/Application Support/Claude/claude_desktop_config.json`
- **Windows**: `%APPDATA%\Claude\claude_desktop_config.json`

The file uses a single `mcpServers` object; each entry is a local stdio server:

```json
{
  "mcpServers": {
    "filesystem": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "/Users/username/Desktop"]
    }
  }
}
```

| Field     | Required | Description                                      |
| --------- | -------- | ------------------------------------------------ |
| `command` | Yes      | Executable to run (e.g. `npx`, `node`, `python`) |
| `args`    | No       | Arguments passed to the command                  |
| `env`     | No       | Environment variables for the server             |

Paths in `args` must be absolute. Save the file, then completely quit and restart Claude Desktop — the config is only read at startup. Verify the server under the input box → **Connectors → Manage connectors**.

> **No `url` transport in this file**: `claude_desktop_config.json` supports stdio servers only. A remote/streamable-HTTP server cannot be connected with a `url` entry here.

## Connecting an HTTP MCP server

Two options:

1. **Connectors (built-in)**: Remote servers are added as Connectors in the app (Settings → Connectors, or a custom connector from the input box → Connectors → Add custom connector). This is the supported path for hosted MCP servers and handles OAuth in-app.
2. **`mcp-remote` bridge (local HTTP servers)**: bridge the HTTP endpoint to stdio with the community-standard `mcp-remote` package:

```json
{
  "mcpServers": {
    "moor": {
      "command": "npx",
      "args": ["-y", "mcp-remote", "http://127.0.0.1:9223/mcp"]
    }
  }
}
```

Requires Node.js on the machine. Headers can be passed as `--header "Name: Value"` arguments to `mcp-remote`; for credentials prefer `--header-file <path>` so secrets stay out of the process arguments.

## Troubleshooting

- Restart Claude Desktop completely after every config change
- Check the JSON syntax; invalid files silently drop servers
- MCP logs live in `~/Library/Logs/Claude` (macOS) or `%APPDATA%\Claude\logs` (Windows): `mcp.log` for connection issues, `mcp-server-<name>.log` for a server's stderr
- Run the server's command manually in a terminal to see startup errors
- On Windows, an `ENOENT` error mentioning `${APPDATA}` means the `env` block needs an explicit `APPDATA` entry

## Security

- Servers run with your user permissions — only grant access to directories and services you're comfortable with Claude reading and modifying
- Every tool action requires your explicit approval; review requests before approving
- Only configure servers from trusted sources

## Moor integration boundary

The stdio bridge is third-party software: https://github.com/punkpeye/mcp-remote. It requires Node.js; Moor does not install it. Credential references in bridge headers require values in the entry's `env` block because GUI clients do not inherit the terminal environment reliably. Moor models the macOS path; Windows remains documented but is not connection-tested in this change.
