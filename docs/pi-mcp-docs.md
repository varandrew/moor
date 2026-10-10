# Pi Coding Agent Model Context Protocol (MCP)

> Source: https://pi.dev/docs/latest/mcp
> Additional sources: https://pi.dev/docs/latest/cli#mcp-commands, https://pi.dev/packages/pi-mcp-adapter
> Checked: 2026-10-08
> Applicable version: Native MCP in Pi 0.99 and later; current rolling documentation.

## Configuration

Native MCP needs no adapter. User configuration: `~/.pi/agent/mcp.json`; project configuration: `.pi/mcp.json`. Project configuration requires trust and overrides same-name user entries.

### Stdio server

```json
{
  "mcpServers": {
    "local-tools": {
      "command": "node",
      "args": ["/absolute/path/server.js"],
      "cwd": "/absolute/path/project",
      "env": { "API_KEY": "${API_KEY}" }
    }
  }
}
```

### Connecting Moor

```json
{
  "mcpServers": {
    "moor": { "url": "http://127.0.0.1:9223/mcp" }
  }
}
```

### Fields

| Field                                     | Meaning                                                    |
| ----------------------------------------- | ---------------------------------------------------------- |
| `command`, `args`, `env`, `cwd`           | Local process configuration                                |
| `url`, `headers`, `oauth`                 | HTTP connection and credentials                            |
| `type`                                    | Optional: `stdio`, `http`, `streamable-http`; SSE rejected |
| `enabled`                                 | `false` disables connection                                |
| `timeout`                                 | Request timeout in seconds; default 60                     |
| `description`, `exposure`, `toolExposure` | Discovery and tool visibility                              |

`env` and `headers` support `${VAR}`. Command-valued credentials are client features; Moor never executes them during conversion.

## Managing servers

```bash
pi mcp add moor --url http://127.0.0.1:9223/mcp
pi mcp list
```

Use `/mcp` for connection status; `/reload` after external edits.

## Adapter compatibility

The community adapter can replace native session support. Its fields, including `bearerTokenEnv`, differ. Moor targets native MCP; adapter-only fields are not converted.

## Troubleshooting and security

Check URLs, credentials and trust. Configure trusted servers only. Moor rejects SSE export to Pi and reports skipped entries.
