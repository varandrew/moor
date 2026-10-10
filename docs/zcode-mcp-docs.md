# ZCode Model Context Protocol (MCP)

> Source: https://zcode.z.ai/en/docs/mcp-services
> Additional sources: https://github.com/zai-org/ZCode/blob/main/apps/zcode-cli/README.md
> Checked: 2026-10-08
> Applicable version: Current Desktop documentation and CLI main branch; their disabled-field conventions differ.

## Configuration

| Scope         | Path                                               | Key           |
| ------------- | -------------------------------------------------- | ------------- |
| User          | `~/.zcode/cli/config.json`                         | `mcp.servers` |
| Project       | `.zcode/config.json`                               | `mcp.servers` |
| Compatibility | `~/.agents/mcp.json` or project `.agents/mcp.json` | `mcpServers`  |

Native entries suppress the compatibility file within that scope, including disabled entries. User entries win same-name project conflicts. Moor scans user files only.

### Stdio server

```json
{
  "mcp": {
    "servers": {
      "local-tools": { "type": "stdio", "command": "node", "args": ["/absolute/path/server.js"] }
    }
  }
}
```

### Connecting Moor

```json
{
  "mcp": {
    "servers": {
      "moor": { "type": "http", "url": "http://127.0.0.1:9223/mcp" }
    }
  }
}
```

### Fields

| Field                    | Meaning                                  |
| ------------------------ | ---------------------------------------- |
| `type`                   | `stdio`, `http`, `sse`                   |
| `command`, `args`, `env` | Local process                            |
| `url`, `headers`         | Remote endpoint and credentials          |
| `timeoutMs`              | CLI request timeout, milliseconds        |
| `enabled` / `enable`     | CLI / Desktop disabled-state conventions |

Moor skips entries when either disabled-state field is `false`; exports omit these fields. Native header interpolation is unverified, so references are preserved with a warning.

## Managing servers

Use Settings → MCP Servers to add, enable or test a connection. Full configuration mode accepts `mcpServers` JSON; saved native files use the nested structure above.

## Troubleshooting and security

Check the native/fallback distinction before diagnosing missing servers. Review project configurations before opening untrusted repositories. Moor never executes credential helpers while importing.
