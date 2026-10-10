# Connect Claude Code to tools via MCP

> Source: https://code.claude.com/docs/en/mcp
> Checked: 2026-10-08
> Applicable version: Current official documentation; version-specific requirements are noted below.
>
> Note: This document is an external documentation mirror/reference. Copyright belongs to the original site; content may be outdated, please refer to the official link. Follow the original site's license when citing or redistributing.

Claude Code can connect to hundreds of external tools and data sources through the Model Context Protocol (MCP), an open source standard for AI-tool integrations. MCP servers give Claude Code access to your tools, databases, and APIs.

Connect a server when you find yourself copying data into chat from another tool, like an issue tracker or a monitoring dashboard. Once connected, Claude can read and act on that system directly instead of working from what you paste.

## What you can do with MCP

With MCP servers connected, you can ask Claude Code to:

- **Implement features from issue trackers**: "Add the feature described in JIRA issue ENG-4521 and create a PR on GitHub."
- **Analyze monitoring data**: "Check Sentry and Statsig to check the usage of the feature described in ENG-4521."
- **Query databases**: "Find emails of 10 random users who used feature ENG-4521, based on our PostgreSQL database."
- **Integrate designs**: "Update our standard email template based on the new Figma designs that were posted in Slack"
- **Automate workflows**: "Create Gmail drafts inviting these 10 users to a feedback session about the new feature."
- **React to external events**: An MCP server can also act as a channel that pushes messages into your session, so Claude reacts to Telegram messages, Discord chats, or webhook events while you're away.

## Popular MCP servers

Here are some commonly used MCP servers you can connect to Claude Code:

## Installing MCP servers

MCP servers can be configured in three different ways depending on your needs:

### Option 1: Add a remote HTTP server

HTTP servers are the recommended option for connecting to remote MCP servers. This is the most widely supported transport for cloud-based services.

```bash
# Basic syntax
claude mcp add --transport http <name> <url>

# Real example: Connect to Notion
claude mcp add --transport http notion https://mcp.notion.com/mcp

# Example with Bearer token
claude mcp add --transport http secure-api https://api.example.com/mcp \
  --header "Authorization: Bearer your-token"
```

### Option 2: Add a remote SSE server

```bash
# Basic syntax
claude mcp add --transport sse <name> <url>

# Real example: Connect to Asana
claude mcp add --transport sse asana https://mcp.asana.com/sse

# Example with authentication header
claude mcp add --transport sse private-api https://api.company.com/sse \
  --header "X-API-Key: your-key-here"
```

### Option 3: Add a local stdio server

Stdio servers run as local processes on your machine. They're ideal for tools that need direct system access or custom scripts.

```bash
# Basic syntax
claude mcp add [options] <name> -- <command> [args...]

# Real example: Add Airtable server
claude mcp add --transport stdio --env AIRTABLE_API_KEY=YOUR_KEY airtable \
  -- npx -y airtable-mcp-server
```

### Managing your servers

Once configured, you can manage your MCP servers with these commands:

```bash
# List all configured servers (with health status)
claude mcp list

# Get details for a specific server
claude mcp get github

# Remove a server
claude mcp remove github

# Authenticate with an OAuth server from the shell (v2.1.186+)
claude mcp login my-server

# Clear stored OAuth credentials
claude mcp logout my-server

# Reset .mcp.json approval choices for the current project
claude mcp reset-project-choices

# (within Claude Code) Check server status
/mcp
```

Server names may only contain letters, numbers, hyphens, and underscores. The names `workspace`, `claude-in-chrome`, `computer-use`, `Claude Preview`, and `Claude Browser` are reserved and skipped with a warning.

### Dynamic tool updates

Claude Code supports MCP `list_changed` notifications, allowing MCP servers to dynamically update their available tools, prompts, and resources without requiring you to disconnect and reconnect. When an MCP server sends a `list_changed` notification, Claude Code automatically refreshes the available capabilities from that server.

### Automatic reconnection

If an HTTP or SSE server disconnects mid-session, Claude Code automatically reconnects with exponential backoff: up to five attempts, starting at a one-second delay and doubling each time. The server appears as pending in `/mcp` while reconnection is in progress. After five failed attempts the server is marked as failed and you can retry manually from `/mcp`. Stdio servers are local processes and are not reconnected automatically.

The same backoff applies when an HTTP or SSE server fails its initial connection at startup. As of v2.1.121, Claude Code retries the initial connection up to three times on transient errors such as a 5xx response, a connection refused, or a timeout, then marks the server as failed if it still cannot connect. Authentication and not-found errors are not retried because they require a configuration change to resolve.

### Push messages with channels

An MCP server can also push messages directly into your session so Claude can react to external events like CI results, monitoring alerts, or chat messages. To enable this, your server declares the `claude/channel` capability and you opt it in with the `--channels` flag at startup. See Channels to use an officially supported channel, or Channels reference to build your own.

### Plugin-provided MCP servers

Plugins can bundle MCP servers, automatically providing tools and integrations when the plugin is enabled. Plugin MCP servers work identically to user-configured servers.

**How plugin MCP servers work**:

- Plugins define MCP servers in `.mcp.json` at the plugin root or inline in `plugin.json`
- When a plugin is enabled, its MCP servers start automatically
- Plugin MCP tools appear alongside manually configured MCP tools
- Plugin servers are managed through plugin installation (not `/mcp` commands)

**Example plugin MCP configuration**:

In `.mcp.json` at plugin root:

```json
{
  "mcpServers": {
    "database-tools": {
      "command": "${CLAUDE_PLUGIN_ROOT}/servers/db-server",
      "args": ["--config", "${CLAUDE_PLUGIN_ROOT}/config.json"],
      "env": {
        "DB_URL": "${DB_URL}"
      }
    }
  }
}
```

Or inline in `plugin.json`:

```json
{
  "name": "my-plugin",
  "mcpServers": {
    "plugin-api": {
      "command": "${CLAUDE_PLUGIN_ROOT}/servers/api-server",
      "args": ["--port", "8080"]
    }
  }
}
```

**Plugin MCP features**:

- **Automatic lifecycle**: At session startup, servers for enabled plugins connect automatically. If you enable or disable a plugin during a session, run `/reload-plugins` to connect or disconnect its MCP servers
- **Environment variables**: use `${CLAUDE_PLUGIN_ROOT}` for bundled plugin files and `${CLAUDE_PLUGIN_DATA}` for persistent state that survives plugin updates
- **User environment access**: Access to same environment variables as manually configured servers
- **Multiple transport types**: Support stdio, SSE, and HTTP transports (transport support may vary by server)

**Viewing plugin MCP servers**:

```bash
# Within Claude Code, see all MCP servers including plugin ones
/mcp
```

Plugin servers appear in the list with indicators showing they come from plugins.

**Benefits of plugin MCP servers**:

- **Bundled distribution**: Tools and servers packaged together
- **Automatic setup**: No manual MCP configuration needed
- **Team consistency**: Everyone gets the same tools when plugin is installed

## MCP installation scopes

MCP servers can be configured at three scopes. The scope you choose controls which projects the server loads in and whether the configuration is shared with your team.

| Scope   | Loads in             | Shared with team         | Stored in                   |
| ------- | -------------------- | ------------------------ | --------------------------- |
| Local   | Current project only | No                       | `~/.claude.json`            |
| Project | Current project only | Yes, via version control | `.mcp.json` in project root |
| User    | All your projects    | No                       | `~/.claude.json`            |

### Local scope

Local scope is the default. A local-scoped server loads only in the project where you added it and stays private to you. Claude Code stores it in `~/.claude.json` under that project's path, so the same server won't appear in your other projects. Use local scope for personal development servers, experimental configurations, or servers with credentials you don't want in version control.

```bash
# Add a local-scoped server (default)
claude mcp add --transport http stripe https://mcp.stripe.com

# Explicitly specify local scope
claude mcp add --transport http stripe --scope local https://mcp.stripe.com
```

The command writes the server into the entry for your current project inside `~/.claude.json`. The example below shows the result when you run it from `/path/to/your/project`:

```json
{
  "projects": {
    "/path/to/your/project": {
      "mcpServers": {
        "stripe": {
          "type": "http",
          "url": "https://mcp.stripe.com"
        }
      }
    }
  }
}
```

### Project scope

Project-scoped servers enable team collaboration by storing configurations in a `.mcp.json` file at your project's root directory. This file is designed to be checked into version control, ensuring all team members have access to the same MCP tools and services. When you add a project-scoped server, Claude Code automatically creates or updates this file with the appropriate configuration structure.

```bash
# Add a project-scoped server
claude mcp add --transport http paypal --scope project https://mcp.paypal.com/mcp
```

The resulting `.mcp.json` file follows a standardized format:

```json
{
  "mcpServers": {
    "shared-server": {
      "command": "/path/to/server",
      "args": [],
      "env": {}
    }
  }
}
```

For security reasons, Claude Code prompts for approval before using project-scoped servers from `.mcp.json` files. If you need to reset these approval choices, use the `claude mcp reset-project-choices` command.

### User scope

User-scoped servers are stored in `~/.claude.json` and provide cross-project accessibility, making them available across all projects on your machine while remaining private to your user account. This scope works well for personal utility servers, development tools, or services you frequently use across different projects.

```bash
# Add a user server
claude mcp add --transport http hubspot --scope user https://mcp.hubspot.com/anthropic
```

### Scope hierarchy and precedence

When the same server is defined in more than one place, Claude Code connects to it once, using the definition from the highest-precedence source:

1. Local scope
2. Project scope
3. User scope
4. Plugin-provided servers
5. claude.ai connectors

The three scopes match duplicates by name. Plugins and connectors match by endpoint, so one that points at the same URL or command as a server above is treated as a duplicate.

### Environment variable expansion in `.mcp.json`

Claude Code supports environment variable expansion in `.mcp.json` files, allowing teams to share configurations while maintaining flexibility for machine-specific paths and sensitive values like API keys.

**Supported syntax:**

- `${VAR}` - Expands to the value of environment variable `VAR`
- `${VAR:-default}` - Expands to `VAR` if set, otherwise uses `default`

**Expansion locations:**

Environment variables can be expanded in:

- `command` - The server executable path
- `args` - Command-line arguments
- `env` - Environment variables passed to the server
- `url` - For HTTP server types
- `headers` - For HTTP server authentication

**Example with variable expansion:**

```json
{
  "mcpServers": {
    "api-server": {
      "type": "http",
      "url": "${API_BASE_URL:-https://api.example.com}/mcp",
      "headers": {
        "Authorization": "Bearer ${API_KEY}"
      }
    }
  }
}
```

Unset variables without defaults remain literal `${VAR}` references and produce a warning. Protected credential variables may be cleared in remote URLs and headers; consult the official security rules.

## Practical examples

### Example: Monitor errors with Sentry

```bash
claude mcp add --transport http sentry https://mcp.sentry.dev/mcp
```

Authenticate with your Sentry account:

Then debug production issues:

```
What are the most common errors in the last 24 hours?
```

```
Show me the stack trace for error ID abc123
```

```
Which deployment introduced these new errors?
```

### Example: Connect to GitHub for code reviews

GitHub's remote MCP server authenticates with a GitHub personal access token passed as a header. To get one, open your GitHub token settings, generate a new fine-grained token with access to the repositories you want Claude to work with, then add the server:

```bash
claude mcp add --transport http github https://api.githubcopilot.com/mcp/ \
  --header "Authorization: Bearer YOUR_GITHUB_PAT"
```

Then work with GitHub:

```
Review PR #456 and suggest improvements
```

```
Create a new issue for the bug we just found
```

```
Show me all open PRs assigned to me
```

### Example: Query your PostgreSQL database

```bash
claude mcp add --transport stdio db -- npx -y @bytebase/dbhub \
  --dsn "postgresql://readonly:pass@prod.db.com:5432/analytics"
```

Then query your database naturally:

```
What's our total revenue this month?
```

```
Show me the schema for the orders table
```

```
Find customers who haven't made a purchase in 90 days
```

## Authenticate with remote MCP servers

Many cloud-based MCP servers require authentication. Claude Code supports OAuth 2.0 for secure connections.

### Use a fixed OAuth callback port

Some MCP servers require a specific redirect URI registered in advance. By default, Claude Code picks a random available port for the OAuth callback. Use `--callback-port` to fix the port so it matches a pre-registered redirect URI of the form `http://localhost:PORT/callback`.

You can use `--callback-port` on its own (with dynamic client registration) or together with `--client-id` (with pre-configured credentials).

```bash
# Fixed callback port with dynamic client registration
claude mcp add --transport http \
  --callback-port 8080 \
  my-server https://mcp.example.com/mcp
```

### Use pre-configured OAuth credentials

Some MCP servers don't support automatic OAuth setup via Dynamic Client Registration. If you see an error like "Incompatible auth server: does not support dynamic client registration," the server requires pre-configured credentials. Claude Code also supports servers that use a Client ID Metadata Document (CIMD) instead of Dynamic Client Registration, and discovers these automatically. If automatic discovery fails, register an OAuth app through the server's developer portal first, then provide the credentials when adding the server.

### Override OAuth metadata discovery

Point Claude Code at a specific OAuth authorization server metadata URL to bypass the default discovery chain. Set `authServerMetadataUrl` when the MCP server's standard endpoints error, or when you want to route discovery through an internal proxy. By default, Claude Code first checks RFC 9728 Protected Resource Metadata at `/.well-known/oauth-protected-resource`, then falls back to RFC 8414 authorization server metadata at `/.well-known/oauth-authorization-server`.

Set `authServerMetadataUrl` in the `oauth` object of your server's config in `.mcp.json`:

```json
{
  "mcpServers": {
    "my-server": {
      "type": "http",
      "url": "https://mcp.example.com/mcp",
      "oauth": {
        "authServerMetadataUrl": "https://auth.example.com/.well-known/openid-configuration"
      }
    }
  }
}
```

The URL must use `https://`. `authServerMetadataUrl` requires Claude Code v2.1.64 or later. The metadata URL's `scopes_supported` overrides the scopes the upstream server advertises.

### Restrict OAuth scopes

Set `oauth.scopes` to pin the scopes Claude Code requests during the authorization flow. This is the supported way to restrict an MCP server to a security-team-approved subset when the upstream authorization server advertises more scopes than you want to grant. The value is a single space-separated string, matching the `scope` parameter format in RFC 6749 §3.3.

```json
{
  "mcpServers": {
    "slack": {
      "type": "http",
      "url": "https://mcp.slack.com/mcp",
      "oauth": {
        "scopes": "channels:read chat:write search:read"
      }
    }
  }
}
```

`oauth.scopes` takes precedence over both `authServerMetadataUrl` and the scopes the server discovers at `/.well-known`. Leave it unset to let the MCP server determine the requested scope set.

If the authorization server advertises `offline_access` in `scopes_supported`, Claude Code appends it to the pinned scopes so the access token can be refreshed without a new browser sign-in.

If the server later returns a 403 `insufficient_scope` for a tool call, Claude Code re-authenticates with the same pinned scopes. Widen `oauth.scopes` when a tool you need requires a scope outside the pin.

### Use headersHelper for non-OAuth authentication

If your MCP server uses an authentication scheme other than OAuth (such as Kerberos, short-lived tokens, or an internal SSO), use `headersHelper` to generate request headers at connection time. Claude Code runs the command and merges its output into the connection headers.

```json
{
  "mcpServers": {
    "internal-api": {
      "type": "http",
      "url": "https://mcp.internal.example.com",
      "headersHelper": "/opt/bin/get-mcp-auth-headers.sh"
    }
  }
}
```

The command can also be inline:

```json
{
  "mcpServers": {
    "internal-api": {
      "type": "http",
      "url": "https://mcp.internal.example.com",
      "headersHelper": "echo '{\"Authorization\": \"Bearer '\"$(get-token)\"'\"}'"
    }
  }
}
```

**Requirements:**

- The command must write a JSON object of string key-value pairs to stdout
- The command runs in a shell with a 10-second timeout
- Dynamic headers override any static `headers` with the same name

The helper runs fresh on each connection (at session start and on reconnect). There is no caching, so your script is responsible for any token reuse.

Claude Code sets these environment variables when executing the helper:

| Variable                      | Value                      |
| ----------------------------- | -------------------------- |
| `CLAUDE_CODE_MCP_SERVER_NAME` | the name of the MCP server |
| `CLAUDE_CODE_MCP_SERVER_URL`  | the URL of the MCP server  |

Use these to write a single helper script that serves multiple MCP servers.

## Add MCP servers from JSON configuration

If you have a JSON configuration for an MCP server, you can add it directly with `claude mcp add-json`:

```bash
# Pass the object INSIDE mcpServers, not the wrapper
claude mcp add-json example '{"command":"npx","args":["-y","@example/mcp-server"]}'

# WebSocket servers are JSON-only (--transport does not accept "ws")
claude mcp add-json events-server \
  '{"type":"ws","url":"wss://mcp.example.com/socket","headers":{"Authorization":"Bearer YOUR_TOKEN"}}'
```

**Entry `type` rules**: the `type` field accepts `http` (with `streamable-http` as an alias), `sse`, `ws`, or `stdio`. A JSON entry that has a `url` but no `type` is a configuration error — entries without a `type` are read as stdio and fail with a missing-`command` error, so repair `url`-only entries by adding `"type": "http"` (or `"sse"` / `"ws"`).

## Import MCP servers from Claude Desktop

If you've already configured MCP servers in Claude Desktop, import them with:

```bash
claude mcp add-from-claude-desktop
```

This is available on macOS and WSL. Imported names that Claude Desktop rejects (invalid characters) are skipped; duplicates get numeric suffixes (`server_1`).

## Use MCP servers from Claude.ai

If you've logged into Claude Code with a Claude.ai account, MCP servers you've added in Claude.ai are automatically available in Claude Code.

A server you've added in Claude Code takes precedence over a claude.ai connector that points at the same URL. When this happens, `/mcp` lists the connector as hidden and shows how to remove the duplicate if you'd rather use the connector.

To disable claude.ai MCP servers in Claude Code, set the `ENABLE_CLAUDEAI_MCP_SERVERS` environment variable to `false`:

```bash
ENABLE_CLAUDEAI_MCP_SERVERS=false claude
```

## Use Claude Code as an MCP server

You can use Claude Code itself as an MCP server that other applications can connect to:

```bash
# Start Claude as a stdio MCP server
claude mcp serve
```

You can use this in Claude Desktop by adding this configuration to claude_desktop_config.json:

```json
{
  "mcpServers": {
    "claude-code": {
      "type": "stdio",
      "command": "claude",
      "args": ["mcp", "serve"],
      "env": {}
    }
  }
}
```

## Timeouts

| Mechanism                | Setting                                                 | Notes                                                                                            |
| ------------------------ | ------------------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| Server startup timeout   | `MCP_TIMEOUT` env var (ms)                              | e.g. `MCP_TIMEOUT=10000 claude`                                                                  |
| Per-tool-call wall clock | `timeout` field (ms) in the server entry                | Hard wall-clock limit per tool call; overrides `MCP_TOOL_TIMEOUT`; values below 1000 are ignored |
| Global tool timeout      | `MCP_TOOL_TIMEOUT` env var                              | Defaults to roughly 28 hours when unset                                                          |
| Idle disconnect          | `CLAUDE_CODE_MCP_TOOL_IDLE_TIMEOUT` (ms), `0` disables  | Default 5 min (HTTP/SSE/WS/claude.ai), 30 min (stdio); v2.1.203+                                 |
| Auto-backgrounding       | `CLAUDE_CODE_MCP_AUTO_BACKGROUND_MS` (ms), `0` disables | Tools running longer than 2 min are backgrounded; v2.1.212+                                      |

For HTTP/SSE/claude.ai connectors, each request timer is the greatest of 60s, the server tool timeout, and `MCP_TIMEOUT`.

In stdio `command`/`args` for `.mcp.json`/`~/.claude.json` entries, `${CLAUDE_PROJECT_DIR}` references require a default such as `${CLAUDE_PROJECT_DIR:-.}` (plugin configs substitute it directly).

## MCP output limits and warnings

When MCP tools produce large outputs, Claude Code helps manage the token usage to prevent overwhelming your conversation context:

- **Output warning threshold**: Claude Code displays a warning when any MCP tool output exceeds 10,000 tokens
- **Configurable limit**: you can adjust the maximum allowed MCP output tokens using the `MAX_MCP_OUTPUT_TOKENS` environment variable
- **Default limit**: the default maximum is 25,000 tokens
- **Scope**: the environment variable applies to tools that don't declare their own limit. Tools that set `anthropic/maxResultSizeChars` use that value instead for text content, regardless of what `MAX_MCP_OUTPUT_TOKENS` is set to. Tools that return image data are still subject to `MAX_MCP_OUTPUT_TOKENS`

To increase the limit for tools that produce large outputs:

```bash
export MAX_MCP_OUTPUT_TOKENS=50000
claude
```

This is particularly useful when working with MCP servers that:

- Query large datasets or databases
- Generate detailed reports or documentation
- Process extensive log files or debugging information

### Raise the limit for a specific tool

If you're building an MCP server, you can allow individual tools to return results larger than the default persist-to-disk threshold by setting `_meta["anthropic/maxResultSizeChars"]` in the tool's `tools/list` response entry. Claude Code raises that tool's threshold to the annotated value, up to a hard ceiling of 500,000 characters.

This is useful for tools that return inherently large but necessary outputs, such as database schemas or full file trees. Without the annotation, results that exceed the default threshold are persisted to disk and replaced with a file reference in the conversation.

```json
{
  "name": "get_schema",
  "description": "Returns the full database schema",
  "_meta": {
    "anthropic/maxResultSizeChars": 200000
  }
}
```

The annotation applies independently of `MAX_MCP_OUTPUT_TOKENS` for text content, so users don't need to raise the environment variable for tools that declare it. Tools that return image data are still subject to the token limit.

## Respond to MCP elicitation requests

MCP servers can request structured input from you mid-task using elicitation. When a server needs information it can't get on its own, Claude Code displays an interactive dialog and passes your response back to the server. No configuration is required on your side: elicitation dialogs appear automatically when a server requests them.

Servers can request input in two ways:

- **Form mode**: Claude Code shows a dialog with form fields defined by the server (for example, a username and password prompt). Fill in the fields and submit.
- **URL mode**: Claude Code opens a browser URL for authentication or approval. Complete the flow in the browser, then confirm in the CLI.

To auto-respond to elicitation requests without showing a dialog, use the `Elicitation` hook.

## Use MCP resources

MCP servers can expose resources that you can reference using @ mentions, similar to how you reference files.

## Tool search

Tool search keeps MCP context usage low by deferring tool definitions until Claude needs them. Only tool names load at session start, so adding more MCP servers has minimal impact on your context window.

### How it works

Tool search is enabled by default. MCP tools are deferred rather than loaded into context upfront, and Claude uses a search tool to discover relevant ones when a task needs them. Only the tools Claude actually uses enter context. From your perspective, MCP tools work exactly as before.

If you prefer threshold-based loading, set `ENABLE_TOOL_SEARCH=auto` to load schemas upfront when they fit within 10% of the context window and defer only the overflow.

If you're building an MCP server, the server instructions field becomes more useful with Tool Search enabled. Server instructions help Claude understand when to search for your tools, similar to how skills work.

Add clear, descriptive server instructions that explain:

- What category of tasks your tools handle
- When Claude should search for your tools
- Key capabilities your server provides

Claude Code truncates tool descriptions and server instructions at 2KB each. Keep them concise to avoid truncation, and put critical details near the start.

### Configure tool search

Tool search is enabled by default: MCP tools are deferred and discovered on demand. It is disabled by default on Vertex AI, which does not accept the tool search beta header, and when `ANTHROPIC_BASE_URL` points to a non-first-party host, since most proxies do not forward `tool_reference` blocks. Set `ENABLE_TOOL_SEARCH` explicitly to opt in. This feature requires models that support `tool_reference` blocks: Sonnet 4 and later, or Opus 4 and later. Haiku models do not support tool search.

Control tool search behavior with the `ENABLE_TOOL_SEARCH` environment variable:

| Value      | Behavior                                                                                                                                       |
| ---------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| (unset)    | All MCP tools deferred and loaded on demand. Falls back to loading upfront on Vertex AI or when `ANTHROPIC_BASE_URL` is a non-first-party host |
| `true`     | All MCP tools deferred, including on Vertex AI and for non-first-party `ANTHROPIC_BASE_URL`                                                    |
| `auto`     | Threshold mode: tools load upfront if they fit within 10% of the context window, deferred otherwise                                            |
| `auto:<N>` | Threshold mode with a custom percentage, where `<N>` is 0-100 (e.g., `auto:5` for 5%)                                                          |
| `false`    | All MCP tools loaded upfront, no deferral                                                                                                      |

```bash
# Use a custom 5% threshold
ENABLE_TOOL_SEARCH=auto:5 claude

# Disable tool search entirely
ENABLE_TOOL_SEARCH=false claude
```

Or set the value in your settings.json `env` field.

You can also disable the `ToolSearch` tool specifically:

```json
{
  "permissions": {
    "deny": ["ToolSearch"]
  }
}
```

### Exempt a server from deferral

If a server's tools should always be visible to Claude without a search step, set `alwaysLoad` to `true` in that server's configuration. Every tool from that server then loads into context at session start regardless of the `ENABLE_TOOL_SEARCH` setting. Use this for a small number of tools that Claude needs on every turn, since each upfront tool consumes context that would otherwise be available for your conversation.

The following `.mcp.json` entry exempts one HTTP server while leaving other servers deferred:

```json
{
  "mcpServers": {
    "core-tools": {
      "type": "http",
      "url": "https://mcp.example.com/mcp",
      "alwaysLoad": true
    }
  }
}
```

The `alwaysLoad` field is available on all server types and requires Claude Code v2.1.121 or later. An MCP server can also mark individual tools as always-loaded by including `"anthropic/alwaysLoad": true` in the tool's `_meta` object, which has the same effect for that tool only.

## Use MCP prompts as commands

MCP servers can expose prompts that become available as commands in Claude Code.

## Managed MCP configuration

For organizations that need centralized control over MCP servers, Claude Code supports two configuration options:

1. **Exclusive control with `managed-mcp.json`**: Deploy a fixed set of MCP servers that users cannot modify or extend
2. **Policy-based control with allowlists/denylists**: Allow users to add their own servers, but restrict which ones are permitted

These options allow IT administrators to:

- **Control which MCP servers employees can access**: Deploy a standardized set of approved MCP servers across the organization
- **Prevent unauthorized MCP servers**: Restrict users from adding unapproved MCP servers
- **Disable MCP entirely**: Remove MCP functionality completely if needed

### Option 1: Exclusive control with managed-mcp.json

When you deploy a `managed-mcp.json` file, it takes **exclusive control** over all MCP servers. Users cannot add, modify, or use any MCP servers other than those defined in this file. This is the simplest approach for organizations that want complete control.

System administrators deploy the configuration file to a system-wide directory:

- macOS: `/Library/Application Support/ClaudeCode/managed-mcp.json`
- Linux and WSL: `/etc/claude-code/managed-mcp.json`
- Windows: `C:\Program Files\ClaudeCode\managed-mcp.json`

The `managed-mcp.json` file uses the same format as a standard `.mcp.json` file:

```json
{
  "mcpServers": {
    "github": {
      "type": "http",
      "url": "https://api.githubcopilot.com/mcp/"
    },
    "sentry": {
      "type": "http",
      "url": "https://mcp.sentry.dev/mcp"
    },
    "company-internal": {
      "type": "stdio",
      "command": "/usr/local/bin/company-mcp-server",
      "args": ["--config", "/etc/company/mcp-config.json"],
      "env": {
        "COMPANY_API_URL": "https://internal.company.com"
      }
    }
  }
}
```

### Option 2: Policy-based control with allowlists and denylists

Instead of taking exclusive control, administrators can allow users to configure their own MCP servers while enforcing restrictions on which servers are permitted. This approach uses `allowedMcpServers` and `deniedMcpServers` in the managed settings file.

#### Restriction options

Each entry in the allowlist or denylist can restrict servers in three ways:

1. **By server name** (`serverName`): Matches the configured name of the server
2. **By command** (`serverCommand`): Matches the exact command and arguments used to start stdio servers
3. **By URL pattern** (`serverUrl`): Matches remote server URLs with wildcard support

**Important**: Each entry must have exactly one of `serverName`, `serverCommand`, or `serverUrl`.

#### Example configuration

```json
{
  "allowedMcpServers": [
    { "serverName": "github" },
    { "serverName": "sentry" },
    { "serverCommand": ["npx", "-y", "@modelcontextprotocol/server-filesystem"] },
    { "serverCommand": ["python", "/usr/local/bin/approved-server.py"] },
    { "serverUrl": "https://mcp.company.com/*" },
    { "serverUrl": "https://*.internal.corp/*" }
  ],
  "deniedMcpServers": [
    { "serverName": "dangerous-server" },
    { "serverCommand": ["npx", "-y", "unapproved-package"] },
    { "serverUrl": "https://*.untrusted.com/*" }
  ]
}
```

#### How command-based restrictions work

**Exact matching**:

- Command arrays must match **exactly** - both the command and all arguments in the correct order
- Example: `["npx", "-y", "server"]` will NOT match `["npx", "server"]` or `["npx", "-y", "server", "--flag"]`

**Stdio server behavior**:

- When the allowlist contains **any** `serverCommand` entries, stdio servers **must** match one of those commands
- Stdio servers cannot pass by name alone when command restrictions are present
- This ensures administrators can enforce which commands are allowed to run

**Non-stdio server behavior**:

- Remote servers (HTTP, SSE, WebSocket) use URL-based matching when `serverUrl` entries exist in the allowlist
- If no URL entries exist, remote servers fall back to name-based matching
- Command restrictions do not apply to remote servers

#### How URL-based restrictions work

URL patterns support wildcards using `*` to match any sequence of characters.

**Wildcard examples**:

- `https://mcp.company.com/*` - Allow all paths on a specific domain
- `https://*.example.com/*` - Allow any subdomain of example.com
- `http://localhost:*/*` - Allow any port on localhost

#### Allowlist behavior (`allowedMcpServers`)

- `undefined` (default): No restrictions - users can configure any MCP server
- Empty array `[]`: Complete lockdown - users cannot configure any MCP servers
- List of entries: Users can only configure servers that match by name, command, or URL pattern

#### Denylist behavior (`deniedMcpServers`)

- `undefined` (default): No servers are blocked
- Empty array `[]`: No servers are blocked
- List of entries: Specified servers are explicitly blocked across all scopes

#### Important notes

- **Option 1 and Option 2 can be combined**: If `managed-mcp.json` exists, it has exclusive control and users cannot add servers. Allowlists/denylists still apply to the managed servers themselves.
- **Denylist takes absolute precedence**: If a server matches a denylist entry (by name, command, or URL), it will be blocked even if it's on the allowlist.
- Name-based, command-based, and URL-based restrictions work together: a server passes if it matches **either** a name entry, a command entry, or a URL pattern (unless blocked by denylist).

## Current transport and helper behavior

HTTP connections can fall back to SSE from v2.1.265. Since v2.1.238, `headersHelper` execution follows the configuration origin and workspace trust: project helpers require trust and run from the project directory; user helpers use the user scope. Since v2.1.268, `${VAR}` references in helper headers are redacted in displayed output. Moor imports helper field names as conversion warnings and never runs helpers.
