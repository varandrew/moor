# Codex MCP Configuration Guide

> Source: https://learn.chatgpt.com/docs/extend/mcp?surface=cli (formerly https://developers.openai.com/codex/mcp, now a permanent redirect)
> Checked: 2026-10-08
> Applicable version: Current official documentation; version-specific requirements are noted below.
>
> Note: This document is an external documentation mirror/reference. Copyright belongs to the original site; content may be outdated, please refer to the official link. Follow the original site's license when citing or redistributing.

Model Context Protocol (MCP) connects models with tools and context. Use MCP to give Codex access to third-party documentation, or let it interact with developer tools like browsers and Figma.

Codex supports MCP Servers in both CLI and IDE extensions.

## Server Types

- **STDIO Server**: Runs as a local process (started via command)
  - Supports environment variables
- **Streamable HTTP Server**: Accessed via URL
  - Bearer Token authentication
  - OAuth authentication (run `codex mcp login <server-name>`)
  - `auth = "chatgpt"` reuses the current ChatGPT session token for trusted first-party origins

## Configuration File Location

Codex stores MCP configuration in `config.toml`, co-located with other Codex configurations. The default location is `~/.codex/config.toml`. You can also scope MCP Servers to a project (`.codex/config.toml`, trusted projects only).

CLI and IDE extensions share this configuration. Once configured, you can switch between the two Codex clients without reconfiguring.

Choose one of two configuration methods:

1. **Use CLI**: Run `codex mcp` to add and manage Servers
2. **Edit `config.toml`**: Directly edit `~/.codex/config.toml` (or project-scoped `.codex/config.toml`)

## Configuring via CLI

### Adding an MCP Server

```bash
codex mcp add <server-name> --env VAR1=VALUE1 --env VAR2=VALUE2 -- <stdio server-command>
```

Example — Adding Context7 (free MCP Server providing developer documentation):

```bash
codex mcp add context7 -- npx -y @upstash/context7-mcp
```

### Other CLI Commands

View all available MCP commands:

```bash
codex mcp --help
```

### Terminal UI (TUI)

In the `codex` TUI, use `/mcp` to view active MCP Servers.

## Configuring via config.toml

For finer-grained control, edit `~/.codex/config.toml` (or project-scoped `.codex/config.toml`). In the IDE extension, select **MCP settings** > **Open config.toml** from the gear menu.

Configure each MCP Server with the `[mcp_servers.<server-name>]` table in the configuration file.

### STDIO Server Configuration

| Field                      | Required | Description                                                     |
| -------------------------- | -------- | --------------------------------------------------------------- |
| `command`                  | Yes      | Command to start the Server                                     |
| `args`                     | No       | Arguments passed to the Server                                  |
| `env`                      | No       | Environment variables set for the Server                        |
| `env_vars`                 | No       | Environment variables to allow and forward                      |
| `cwd`                      | No       | Working directory for Server startup                            |
| `experimental_environment` | No       | Set to `remote` to launch stdio Server via remote execution env |

`env_vars` can contain plain variable names or objects with source:

```toml
env_vars = ["LOCAL_TOKEN", { name = "REMOTE_TOKEN", source = "remote" }]
```

String entries and `source = "local"` read from Codex's local environment. `source = "remote"` reads from the remote execution environment, requiring remote MCP stdio.

### Streamable HTTP Server Configuration

| Field                  | Required | Description                                                                                |
| ---------------------- | -------- | ------------------------------------------------------------------------------------------ |
| `url`                  | Yes      | Server URL                                                                                 |
| `auth`                 | No       | `oauth` (default) or `chatgpt` — reuse the current session for trusted first-party origins |
| `bearer_token_env_var` | No       | Name of environment variable containing Bearer Token, sent in `Authorization` header       |
| `http_headers`         | No       | Mapping of header names to static values                                                   |
| `env_http_headers`     | No       | Mapping of header names to environment variable names (values read from env)               |

### General Configuration Options

| Field                         | Default | Description                                                                 |
| ----------------------------- | ------- | --------------------------------------------------------------------------- |
| `startup_timeout_sec`         | `10`    | Server startup timeout (seconds)                                            |
| `tool_timeout_sec`            | `60`    | Tool execution timeout (seconds)                                            |
| `enabled`                     | —       | Set to `false` to disable Server without deleting                           |
| `required`                    | —       | Set to `true` to fail startup if this Server fails to initialize            |
| `enabled_tools`               | —       | Tool whitelist                                                              |
| `disabled_tools`              | —       | Tool blacklist (applied after `enabled_tools`)                              |
| `default_tools_approval_mode` | —       | Per-server default for tool approval: `auto`, `prompt`, `writes`, `approve` |
| `tools.<tool>.approval_mode`  | —       | Per-tool override of the approval mode                                      |

### Tool Approval Modes

`default_tools_approval_mode` values:

- `auto` — approve automatically
- `prompt` — ask before every call
- `writes` — prompt only for tools not marked read-only
- `approve` — treat all calls as pre-approved

```toml
[mcp_servers.chrome_devtools]
url = "http://localhost:3000/mcp"
default_tools_approval_mode = "prompt"

[mcp_servers.chrome_devtools.tools.open]
approval_mode = "approve"
```

### OAuth Callback Configuration

If the OAuth provider requires a fixed callback port, set it at the top level of `config.toml`:

```toml
mcp_oauth_callback_port = 5555
mcp_oauth_callback_url = "https://devbox.example.internal/callback"
```

- When `mcp_oauth_callback_port` is not set, Codex binds to a temporary port
- `mcp_oauth_callback_url` is used as the OAuth `redirect_uri`, while `mcp_oauth_callback_port` is still used as the callback listening port
- Local callback URLs (e.g., `localhost`) bind to the local interface; non-local URLs bind to `0.0.0.0` so the callback is reachable

If the MCP Server declares `scopes_supported`, Codex prioritizes those scopes during OAuth login; otherwise it falls back to scopes configured in `config.toml`.

For providers without Dynamic Client Registration, pre-register a client per server:

```toml
[mcp_servers.example.oauth]
client_id = "my-client"
callback_url = "http://127.0.0.1/callback"
```

Equivalent CLI: `codex mcp add example --url https://mcp.example.com --oauth-client-id my-client`, or `codex mcp login <server-name> --oauth-client-registration cimd|dcr` to pick the registration flow.

### Plugin-Bundled Servers

Plugins may bundle MCP servers; user config cannot set their transport, but controls state and policy via `[plugins."<plugin>@<scope>".mcp_servers.<name>]` with `enabled`, `enabled_tools`, `default_tools_approval_mode`, and per-tool `approval_mode`. Plugin `.mcp.json` OAuth uses camelCase (`clientId`, `callbackUrl`, `callbackPort`).

### Server Instructions

Codex reads the MCP `instructions` field from the initialization response as server-wide guidance; keep the first 512 characters self-contained.

## Complete config.toml Examples

### Context7 (STDIO)

```toml
[mcp_servers.context7]
command = "npx"
args = ["-y", "@upstash/context7-mcp"]
env_vars = ["LOCAL_TOKEN"]

[mcp_servers.context7.env]
MY_ENV_VAR = "MY_ENV_VALUE"
```

### OAuth Callback Override

```toml
# Optional MCP OAuth callback override (for `codex mcp login`)
mcp_oauth_callback_port = 5555
mcp_oauth_callback_url = "https://devbox.example.internal/callback"
```

### Figma (HTTP + Bearer Token)

```toml
[mcp_servers.figma]
url = "https://mcp.figma.com/mcp"
bearer_token_env_var = "FIGMA_OAUTH_TOKEN"
http_headers = { "X-Figma-Region" = "us-east-1" }
```

### Chrome DevTools (Tool Filtering)

```toml
[mcp_servers.chrome_devtools]
url = "http://localhost:3000/mcp"
enabled_tools = ["open", "screenshot"]
disabled_tools = ["screenshot"] # Applied after enabled_tools
startup_timeout_sec = 20
tool_timeout_sec = 45
enabled = true
```

## Common MCP Servers

| Server                                                                                                                                                                                  | Description                                 |
| --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------- |
| [OpenAI Docs MCP](https://developers.openai.com/learn/docs-mcp)                                                                                                                         | Search and read OpenAI developer docs       |
| [Context7](https://github.com/upstash/context7)                                                                                                                                         | Connect to latest developer docs            |
| [Figma Local](https://developers.figma.com/docs/figma-mcp-server/local-server-installation/) / [Remote](https://developers.figma.com/docs/figma-mcp-server/remote-server-installation/) | Access Figma designs                        |
| [Playwright](https://www.npmjs.com/package/@playwright/mcp)                                                                                                                             | Control and inspect browser with Playwright |
| [Chrome Developer Tools](https://github.com/ChromeDevTools/chrome-devtools-mcp/)                                                                                                        | Control and inspect Chrome                  |
| [Sentry](https://docs.sentry.io/product/sentry-mcp/#codex)                                                                                                                              | Access Sentry logs                          |
| [GitHub](https://github.com/github/github-mcp-server)                                                                                                                                   | Manage GitHub (PRs, Issues, etc.)           |

## Current local configuration additions

`http_headers_helper` can provide HTTP credentials locally; Moor does not execute it during import or conversion. `tools.<tool>.output_token_limit` controls per-tool output size. Top-level `mcp_optional_startup_grace_ms` defaults to 1000 milliseconds. These client options are not part of Moor's common connection model and are reported as unmapped fields when applicable.

For credential selection, explicit `http_headers` and `bearer_token_env_var` take precedence over `auth`; `auth = "chatgpt"` uses ChatGPT credentials with OAuth fallback. Refer to the official documentation for current authorization behavior.

In the IDE, open MCP servers → Add server → Save → Restart extension.
