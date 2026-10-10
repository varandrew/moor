# Grok Build Model Context Protocol (MCP)

> Source: https://docs.x.ai/build/features/mcp-servers
> Checked: 2026-10-08
> Applicable version: Current official documentation; version-specific requirements are noted below.
>
> Note: This document is an external documentation mirror/reference. Copyright belongs to the original site; content may be outdated, please refer to the official link. Follow the original site's license when citing or redistributing.

Grok Build is xAI's coding agent. MCP (Model Context Protocol) servers expose external tools to Grok Build; once configured, their tools become available alongside the built-in ones, namespaced as `<server>__<tool>`.

## Configuration

MCP servers are configured in Grok Build's TOML settings files:

- **Global**: `~/.grok/config.toml`, shared across projects
- **Project**: `.grok/config.toml` — Grok walks from the current directory up to the git root reading each `.grok/config.toml`; a project server with the same name as a user one replaces it entirely

Entries live under `[mcp_servers.<name>]` tables. Grok also reads MCP servers from lower-priority vendor files for compatibility — `~/.claude.json`, `.cursor/mcp.json`, and project `.mcp.json`, merged below `config.toml` in priority. Disable a vendor with `[compat.claude] mcps = false` (or the cursor equivalent).

OAuth tokens are stored in `~/.grok/mcp_credentials.json`; server stderr is written to `~/.grok/logs/mcp/<server>.stderr.log`.

### Stdio server (local)

```toml
[mcp_servers.filesystem]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/path/to/dir"]
env = { API_KEY = "${MY_API_KEY}" }
startup_timeout_sec = 30              # default 30
tool_timeout_sec = 60                 # example; defaults differ between official references
```

### HTTP server (remote)

```toml
[mcp_servers.linear]
url = "https://mcp.linear.app/mcp"
headers = { "x-mcp-session-id" = "{{session_id}}" }
```

The website documents stdio and HTTP; the current official repository additionally documents SSE.

### Fields

| Field                 | Applies to | Description                                            |
| --------------------- | ---------- | ------------------------------------------------------ |
| `command`             | stdio      | Executable to spawn                                    |
| `args`                | stdio      | Arguments passed to the command                        |
| `env`                 | stdio      | Environment variables for the child process            |
| `url`                 | http       | Server URL                                             |
| `headers`             | http       | Static request headers                                 |
| `startup_timeout_sec` | both       | Startup timeout in seconds (default `30`)              |
| `tool_timeout_sec`    | both       | Tool-call timeout in seconds; verify installed version |

Variable expansion: Grok expands `${VAR}` (and `${VAR:-default}`) in `url`, `command`, `args`, `env`, and `headers`. Only these two forms are documented — `${env:VAR}` is not expanded (inferred; the official docs list no such syntax). The `{{session_id}}` template is available in headers. Auth headers can also be passed via the `--header` CLI flag; OAuth is handled through a browser flow.

## CLI commands

```bash
# Add a stdio server (everything after -- is passed to the server)
grok mcp add filesystem -- npx -y @modelcontextprotocol/server-filesystem /path/to/dir

# Add an HTTP server
grok mcp add --transport http linear https://mcp.linear.app/mcp

# Add an HTTP server with an auth header (--header is repeatable)
grok mcp add --transport http api https://mcp.example.com/mcp --header "Authorization: Bearer ${API_TOKEN}"

# Write to project scope instead of the user config
grok mcp add --scope project my-server -- npx my-server

grok mcp list            # supports --json
grok mcp remove <name>
grok mcp doctor [name]   # connectivity diagnostics, supports --json
grok inspect             # shows servers and origins
```

In the TUI, `/mcps` opens the MCP tab — Space toggles a server, `r` refreshes, `i` authenticates OAuth, `a` adds, `x` removes.

## Troubleshooting

First step is `grok mcp doctor`. Cold-start `npx` servers may need a raised `startup_timeout_sec`.

## Security

- Only connect to servers from trusted sources; stdio entries execute local commands
- Review the transport and launch target before enabling project-level servers from repositories you don't own

## Current repository additions

Additional source: https://github.com/xai-org/grok-build/blob/main/crates/codegen/xai-grok-pager/docs/user-guide/07-mcp-servers.md

The website was last updated 2026-07-02. The repository additionally documents `enabled`, `tool_timeouts`, `bearer_token_file`, and `grok mcp enable` / `disable`. Personal disabled names can live in root `disabled_mcp_servers`; Moor respects this list when scanning Grok configuration. The website and repository disagree on the tool timeout default (6000 versus 60 seconds); verify your installed version.

```bash
grok mcp add --transport sse legacy https://example.com/sse
```
