# OpenCode MCP Servers

> Source: https://opencode.ai/docs/mcp-servers/
> Checked: 2026-10-08
> Applicable version: Current official documentation; version-specific requirements are noted below.
>
> Note: This document is an external documentation mirror/reference. Copyright belongs to the original site; content may be outdated, please refer to the official link. Follow the original site's license when citing or redistributing.

You can use **Model Context Protocol (MCP)** to add external tools to OpenCode. OpenCode supports both local and remote servers.

Once added, MCP tools are automatically made available to the LLM alongside built-in tools.

## Caveats

Using MCP servers increases context consumption, and if there are too many tools it can accumulate quickly. It is recommended to choose MCP servers carefully.

Some MCP servers (such as the GitHub MCP server) add a large number of tokens, which can easily exceed context limits.

## Enable

Define MCP servers in the `mcp` field of [OpenCode Config](https://opencode.ai/docs/config/), using a unique name for each MCP. You can reference this MCP by name in prompts.

```json
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "name-of-mcp-server": {
      "enabled": true
    },
    "name-of-other-mcp-server": {}
  }
}
```

You can disable a server by setting `enabled` to `false`, which is useful for temporarily disabling without deleting the configuration.

### Overriding Remote Defaults

Organizations can provide default MCP servers via the `.well-known/opencode` endpoint. These servers may be disabled by default, allowing users to enable them on demand.

Enable a specific server by adding `enabled: true` in the local configuration:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "jira": {
      "type": "remote",
      "url": "https://jira.example.com/mcp",
      "enabled": true
    }
  }
}
```

Local configuration values override remote defaults.

## Local

Add a local MCP server with `type` set to `"local"`:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "my-local-mcp-server": {
      "type": "local",
      "command": ["npx", "-y", "my-mcp-command"],
      "enabled": true,
      "environment": {
        "MY_ENV_VAR": "my_env_var_value"
      }
    }
  }
}
```

`command` can also use `["bun", "x", "my-mcp-command"]`.

Example — Adding `@modelcontextprotocol/server-everything` for testing:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "mcp_everything": {
      "type": "local",
      "command": ["npx", "-y", "@modelcontextprotocol/server-everything"]
    }
  }
}
```

Use it in prompts by adding `use the mcp_everything tool`:

```
use the mcp_everything tool to add the number 3 and 4
```

### Local Options

| Option        | Type    | Required | Description                                                                   |
| ------------- | ------- | -------- | ----------------------------------------------------------------------------- |
| `type`        | String  | Yes      | Must be `"local"`                                                             |
| `command`     | Array   | Yes      | Command and arguments to start the MCP server                                 |
| `environment` | Object  |          | Environment variables set at runtime                                          |
| `cwd`         | String  |          | Working directory for the server; relative paths resolve from the workspace   |
| `enabled`     | Boolean |          | Whether to enable the MCP server on startup                                   |
| `timeout`     | Number  |          | Timeout for fetching tools from the MCP server (ms), default 5000 (5 seconds) |

## Remote

Add a remote MCP server by setting `type` to `"remote"`:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "my-remote-mcp": {
      "type": "remote",
      "url": "https://my-mcp-server.com",
      "enabled": true,
      "headers": {
        "Authorization": "Bearer MY_API_KEY"
      }
    }
  }
}
```

### Remote Options

| Option    | Type    | Required | Description                                                                   |
| --------- | ------- | -------- | ----------------------------------------------------------------------------- |
| `type`    | String  | Yes      | Must be `"remote"`                                                            |
| `url`     | String  | Yes      | URL of the remote MCP server                                                  |
| `enabled` | Boolean |          | Whether to enable the MCP server on startup                                   |
| `headers` | Object  |          | Headers sent with requests                                                    |
| `oauth`   | Object  |          | OAuth authentication configuration, see OAuth section below                   |
| `timeout` | Number  |          | Timeout for fetching tools from the MCP server (ms), default 5000 (5 seconds) |

## OAuth

OpenCode automatically handles OAuth authentication for remote MCP servers. When a server requires authentication, OpenCode will:

1. Detect 401 responses and initiate the OAuth flow
2. Use **Dynamic Client Registration (RFC 7591)** if the server supports it
3. Securely store tokens for subsequent requests

### Automatic

Most OAuth-enabled MCP servers require no special configuration:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "my-oauth-server": {
      "type": "remote",
      "url": "https://mcp.example.com/mcp"
    }
  }
}
```

If the server requires authentication, OpenCode will prompt you to authenticate on first use. You can also trigger it manually: `opencode mcp auth <server-name>`.

### Pre-registered

If you have client credentials from the MCP server provider, you can pre-configure them:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "my-oauth-server": {
      "type": "remote",
      "url": "https://mcp.example.com/mcp",
      "oauth": {
        "clientId": "{env:MY_MCP_CLIENT_ID}",
        "clientSecret": "{env:MY_MCP_CLIENT_SECRET}",
        "scope": "tools:read tools:execute"
      }
    }
  }
}
```

### Authenticating

Manually trigger authentication or manage credentials:

```bash
# Authenticate with a specific MCP server
opencode mcp auth my-oauth-server

# List all MCP servers and their authentication status
opencode mcp list

# Remove stored credentials
opencode mcp logout my-oauth-server
```

The `mcp auth` command opens a browser for authorization. After authorization, OpenCode securely stores the token in `~/.local/share/opencode/mcp-auth.json`.

### Disabling OAuth

To disable automatic OAuth for a server (e.g., a server using API Key), set `oauth` to `false`:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "my-api-key-server": {
      "type": "remote",
      "url": "https://mcp.example.com/mcp",
      "oauth": false,
      "headers": {
        "Authorization": "Bearer {env:MY_API_KEY}"
      }
    }
  }
}
```

### OAuth Options

| Option         | Type           | Description                                                            |
| -------------- | -------------- | ---------------------------------------------------------------------- |
| `oauth`        | Object / false | OAuth configuration object, or `false` to disable OAuth auto-detection |
| `clientId`     | String         | OAuth client ID; attempts dynamic client registration if not provided  |
| `clientSecret` | String         | OAuth client secret (if required by the authorization server)          |
| `scope`        | String         | OAuth scopes requested during authorization                            |

### Debugging

When remote MCP server authentication fails, use the following commands to diagnose:

```bash
# View authentication status for all OAuth servers
opencode mcp auth list

# Debug connection and OAuth flow for a specific server
opencode mcp debug my-oauth-server
```

The `mcp debug` command displays the current authentication status, tests the HTTP connection, and attempts the OAuth discovery flow.

## Manage

MCP is available as tools in OpenCode, alongside built-in tools. You can manage them through OpenCode configuration just like other tools.

### Global

Globally enable or disable MCP tools:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "my-mcp-foo": {
      "type": "local",
      "command": ["bun", "x", "my-mcp-command-foo"]
    },
    "my-mcp-bar": {
      "type": "local",
      "command": ["bun", "x", "my-mcp-command-bar"]
    }
  },
  "tools": {
    "my-mcp-foo": false
  }
}
```

Use glob patterns to disable in bulk:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "my-mcp-foo": {
      "type": "local",
      "command": ["bun", "x", "my-mcp-command-foo"]
    },
    "my-mcp-bar": {
      "type": "local",
      "command": ["bun", "x", "my-mcp-command-bar"]
    }
  },
  "tools": {
    "my-mcp*": false
  }
}
```

The `my-mcp*` pattern matches all tools like `my-mcp_search`, `my-mcp_list`, etc.

### Per Agent

When there are many MCP servers, you can disable them globally and enable per Agent:

1. Disable the tool globally
2. Enable it in [agent config](https://opencode.ai/docs/agents/#tools)

```json
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "my-mcp": {
      "type": "local",
      "command": ["bun", "x", "my-mcp-command"],
      "enabled": true
    }
  },
  "tools": {
    "my-mcp*": false
  },
  "agent": {
    "my-agent": {
      "tools": {
        "my-mcp*": true
      }
    }
  }
}
```

### Glob Patterns

- `*` matches zero or more arbitrary characters (e.g., `"my-mcp*"` matches `my-mcp_search`, `my-mcp_list`)
- `?` matches exactly one character
- Other characters match literally

## Examples

### Sentry

Add the [Sentry MCP server](https://mcp.sentry.dev/) to interact with Sentry projects and issues:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "sentry": {
      "type": "remote",
      "url": "https://mcp.sentry.dev/mcp",
      "oauth": {}
    }
  }
}
```

After adding the configuration, authenticate with Sentry:

```bash
opencode mcp auth sentry
```

After authentication, you can use Sentry tools in prompts to query issues, projects, and error data:

```
Show me the latest unresolved issues in my project. use sentry
```

### Context7

Add the [Context7 MCP server](https://github.com/upstash/context7) to search documentation:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "context7": {
      "type": "remote",
      "url": "https://mcp.context7.com/mcp"
    }
  }
}
```

Register a free account to use an API Key for higher rate limits:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "context7": {
      "type": "remote",
      "url": "https://mcp.context7.com/mcp",
      "headers": {
        "CONTEXT7_API_KEY": "{env:CONTEXT7_API_KEY}"
      }
    }
  }
}
```

Add `use context7` in prompts, or add a rule in [AGENTS.md](https://opencode.ai/docs/rules/):

```markdown
When you need to search docs, use `context7` tools.
```

### Grep by Vercel

Add the [Grep by Vercel](https://grep.app/) MCP server to search GitHub code snippets:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "gh_grep": {
      "type": "remote",
      "url": "https://mcp.grep.app"
    }
  }
}
```

Use it in prompts:

```
What's the right way to set a custom domain in an SST Astro component? use the gh_grep tool
```

Or add to AGENTS.md:

```markdown
If you are unsure how to do something, use `gh_grep` to search code examples from GitHub.
```

## Configuration locations and precedence

Additional source: https://opencode.ai/docs/config/

User files are `~/.config/opencode/opencode.json` or `.jsonc`; project files are `opencode.json` or `.jsonc`. `OPENCODE_CONFIG` loads between user and project configuration. Overlapping keys are overridden while unrelated keys remain merged. Managed/MDM settings have higher precedence. Moor scans default user locations only; paste project or custom-path files manually.
