# MiniMax Code Model Context Protocol (MCP)

> Source: https://agent.minimax.io/docs/code/agents/mcp
> Additional sources: https://agent.minimax.io/docs/cli/configuration, https://github.com/MiniMax-AI/minimax-code/blob/main/packages/local-runtime-v2/docs/project-mcp.md
> Checked: 2026-10-08
> Applicable version: Current Code/Desktop documentation and shared runtime main branch.

## Configuration

User configuration: `~/.minimax/mcp.json`, under `mcpServers`. Custom data directories use `MINIMAX_DATA_DIR` (then legacy `MAVIS_DATA_DIR`); Moor currently scans the default path only.

Project configuration: `.mcp.json` in the primary working directory; no ancestor search. Runtime priority is ACP session → project → profile. Same-name project entries replace profile entries, including disabled ones. Moor imports project files by paste only.

### Stdio server

```json
{
  "mcpServers": {
    "local-tools": { "type": "stdio", "command": "node", "args": ["/absolute/path/server.js"] }
  }
}
```

### Connecting Moor

```json
{
  "mcpServers": {
    "moor": { "type": "streamable-http", "url": "http://127.0.0.1:9223/mcp" }
  }
}
```

### Fields

| Field                    | Meaning                                   |
| ------------------------ | ----------------------------------------- |
| `type`                   | `stdio`, `http`, `streamable-http`, `sse` |
| `command`, `args`, `env` | Local process                             |
| `url`, `headers`         | Remote connection                         |
| `enabled`                | `false` disables the entry                |
| `description`            | Server-list description                   |
| `timeout`                | Timeout in milliseconds                   |

Configuration supports `${VAR}` and `${VAR:-default}`. Plugin MCP disallows the `http` alias; prefer `streamable-http`. Moor converts connection fields, with warnings for unmapped client options.

## Managing servers

Use MCP Servers in plugin management for configuration and connection tests. With MiniMax Code running:

```bash
mcode mcp list --human
mcode mcp tools moor
```

## Troubleshooting and security

Check JSON, executable availability, enabled state, URL and credentials. Keep secrets out of shared files. Restart the client if tools remain unavailable.
