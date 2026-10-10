# Cursor MCP Documentation

> Source: https://cursor.com/docs/mcp
> Checked: 2026-10-08
> Applicable version: Current official documentation; version-specific requirements are noted below.
>
> Note: This document is an external documentation mirror/reference. Copyright belongs to the original site; content may be outdated, please refer to the official link. Follow the original site's license when citing or redistributing.

## What is MCP?

Model Context Protocol (MCP) enables Cursor to connect to external tools and data sources.

### Why use MCP?

MCP connects Cursor to external systems and data. Instead of explaining your project structure repeatedly, integrate directly with your tools.

Write MCP servers in any language that can print to `stdout` or serve an HTTP endpoint - Python, JavaScript, Go, etc.

Browse official plugins in the Cursor Marketplace. For community plugins and MCP servers, browse [cursor.directory](https://cursor.directory).

### How it works

MCP servers expose capabilities through the protocol, connecting Cursor to external tools or data sources.

Cursor supports three transport methods:

| Transport             | Execution environment | Deployment       | Users          | Input                   | Auth   |
| --------------------- | --------------------- | ---------------- | -------------- | ----------------------- | ------ |
| **`stdio`**           | Local                 | Cursor manages   | Single user    | Shell command           | Manual |
| **`SSE`**             | Local/Remote          | Deploy as server | Multiple users | URL to an SSE endpoint  | OAuth  |
| **`Streamable HTTP`** | Local/Remote          | Deploy as server | Multiple users | URL to an HTTP endpoint | OAuth  |

### Protocol and extension support

Cursor supports these MCP protocol capabilities and extensions:

| Feature              | Support   | Description                                                     |
| -------------------- | --------- | --------------------------------------------------------------- |
| **Tools**            | Supported | Functions for the AI model to execute                           |
| **Prompts**          | Supported | Templated messages and workflows for users                      |
| **Resources**        | Supported | Structured data sources that can be read and referenced         |
| **Roots**            | Supported | Server-initiated inquiries into URI or filesystem boundaries    |
| **Elicitation**      | Supported | Server-initiated requests for additional information from users |
| **Apps (extension)** | Supported | Interactive UI views returned by MCP tools                      |

### MCP apps

Cursor supports the MCP Apps extension. MCP tools can return interactive UI along with standard tool output.

MCP Apps follow progressive enhancement. If a host cannot render app UI, the same tool still works through normal MCP responses.

## Installing MCP servers

### One-click installation

Browse the Cursor Marketplace for official plugins with one-click install. For community plugins and MCP servers, browse [cursor.directory](https://cursor.directory). Click "Add to Cursor" on a marketplace entry to install it and authenticate with OAuth.

### Using `mcp.json`

Configure custom MCP servers with a JSON file:

#### STDIO server example

```json
{
  "mcpServers": {
    "my-server": {
      "type": "stdio",
      "type": "stdio",
      "command": "npx",
      "args": ["-y", "my-mcp-server"],
      "env": {
        "API_KEY": "${env:MY_API_KEY}"
      }
    }
  }
}
```

#### Remote server example (SSE)

```json
{
  "mcpServers": {
    "my-remote-server": {
      "url": "https://my-server.example.com/sse"
    }
  }
}
```

#### Remote server example (Streamable HTTP)

```json
{
  "mcpServers": {
    "my-http-server": {
      "url": "https://my-server.example.com/mcp"
    }
  }
}
```

### Static OAuth for remote servers

For MCP servers that use OAuth, you can provide **static OAuth client credentials** in `mcp.json` instead of dynamic client registration. Use this when:

- The MCP provider gives you a fixed **Client ID** (and optionally **Client Secret**)
- The provider requires **whitelisting a redirect URL** (e.g. Figma, Linear)
- The provider does not support OAuth 2.0 Dynamic Client Registration

Add an `auth` object to remote server entries that use `url`:

```json
{
  "mcpServers": {
    "figma": {
      "url": "https://mcp.figma.com/sse",
      "auth": {
        "CLIENT_ID": "${env:FIGMA_CLIENT_ID}",
        "CLIENT_SECRET": "${env:FIGMA_CLIENT_SECRET}",
        "scopes": ["file_read"]
      }
    }
  }
}
```

| Field             | Required | Description                                                                                                                   |
| ----------------- | -------- | ----------------------------------------------------------------------------------------------------------------------------- |
| **CLIENT_ID**     | Yes      | OAuth 2.0 Client ID from the MCP provider                                                                                     |
| **CLIENT_SECRET** | No       | OAuth 2.0 Client Secret (if the provider uses confidential clients)                                                           |
| **scopes**        | No       | OAuth scopes to request. If omitted, Cursor will use `/.well-known/oauth-authorization-server` to discover `scopes_supported` |

#### Static redirect URLs

Cursor uses fixed OAuth redirect URLs, identified via the OAuth `state` parameter. When configuring the MCP provider's OAuth app, register the applicable URL as an allowed redirect URI:

- Web/agents: `https://www.cursor.com/agents/mcp/oauth/callback`
- Desktop: `http://localhost:8787/callback`

#### Combining with config interpolation

`auth` values support the same interpolation as other fields:

```json
{
  "mcpServers": {
    "my-server": {
      "url": "https://mcp.example.com/mcp",
      "auth": {
        "CLIENT_ID": "${env:MY_CLIENT_ID}",
        "CLIENT_SECRET": "${env:MY_CLIENT_SECRET}"
      }
    }
  }
}
```

Use environment variables for Client ID and Client Secret instead of hardcoding them.

### STDIO server configuration

For STDIO servers (local command-line servers), configure these fields in your `mcp.json`:

| Field       | Required | Description                                                                                                     | Examples                                  |
| ----------- | -------- | --------------------------------------------------------------------------------------------------------------- | ----------------------------------------- |
| **type**    | Yes      | Server connection type                                                                                          | `"stdio"`                                 |
| **command** | Yes      | Command to start the server executable. Must be available on your system path or contain its full path.         | `"npx"`, `"node"`, `"python"`, `"docker"` |
| **args**    | No       | Array of arguments passed to the command                                                                        | `["server.py", "--port", "3000"]`         |
| **env**     | No       | Environment variables for the server                                                                            | `{"API_KEY": "${env:api-key}"}`           |
| **envFile** | No       | Path to an environment file to load more variables. STDIO servers only — remote servers must use interpolation. | `".env"`, `"${workspaceFolder}/.env"`     |

### Using the Extension API

For programmatic MCP server registration, Cursor provides an extension API that allows dynamic configuration without modifying `mcp.json` files. This is particularly useful for enterprise environments and automated setup workflows.

Register MCP servers programmatically using `vscode.cursor.mcp.registerServer()`.

### Configuration locations

MCP configuration files can be placed in several locations:

| Location    | Scope                              | Path                               |
| ----------- | ---------------------------------- | ---------------------------------- |
| **Project** | Shared with team (version control) | `.cursor/mcp.json` in project root |
| **Global**  | Personal, all projects             | `~/.cursor/mcp.json`               |

Both locations are documented. The current MCP reference does not explicitly establish same-name merge precedence; check the client when entries overlap.

### Config interpolation

Use variables in `mcp.json` values. Cursor resolves variables in these fields: `command`, `args`, `env`, `url`, and `headers`.

Supported syntax:

- `${env:NAME}` environment variables
- `${userHome}` path to your home folder
- `${workspaceFolder}` project root (the folder that contains `.cursor/mcp.json`)
- `${workspaceFolderBasename}` name of the project root
- `${pathSeparator}` and `${/}` OS path separator

Examples:

```json
{
  "mcpServers": {
    "database": {
      "type": "stdio",
      "command": "${userHome}/.local/bin/db-server",
      "args": ["--config", "${workspaceFolder}/db-config.json"],
      "env": {
        "DB_URL": "${env:DATABASE_URL}",
        "DATA_DIR": "${workspaceFolder}/data${/}db"
      }
    }
  }
}
```

### Authentication

MCP servers use environment variables for authentication. Pass API keys and tokens through the config.

Cursor supports OAuth for servers that require it.

## Using MCP in chat

Agent automatically uses MCP tools listed under `Available Tools` when relevant. This includes Plan Mode. Ask for a specific tool by name or describe what you need. Enable or disable tools from settings.

### Tool approval

Agent asks for approval before using MCP tools by default. Click the arrow next to the tool name to see arguments.

#### Auto-run

Enable auto-run for Agent to use MCP tools without asking. Works like terminal commands.

To pre-configure which MCP tools can auto-run without using the settings UI, add them to `~/.cursor/permissions.json`:

```json
{
  "allow": ["mcp_server_name.tool_name"]
}
```

### Tool response

Cursor shows the response in chat with expandable views of arguments and responses.

### Images as context

MCP servers can return images - screenshots, diagrams, etc. Return them as base64 encoded strings:

```json
{
  "type": "image",
  "data": "<base64-encoded-image-data>",
  "mimeType": "image/png"
}
```

Cursor attaches returned images to the chat. If the model supports images, it analyzes them.

## Security considerations

When installing MCP servers, consider these security practices:

- **Verify the source**: Only install MCP servers from trusted developers and repositories
- **Review permissions**: Check what data and APIs the server will access
- **Limit API keys**: Use restricted API keys with minimal required permissions
- **Audit code**: For critical integrations, review the server's source code

Remember that MCP servers can access external services and execute code on your behalf. Always understand what a server does before installation.

## Real-world examples

For practical examples of MCP in action:

- **Xcode integration** — Connect Cursor to Xcode 26.3+ for builds, tests, SwiftUI previews, and Apple documentation search
- **Web Development guide** — Integrate Linear, Figma, and browser tools into your development workflow

## FAQ

## Reference notes

The official stdio field table requires `type`, while some examples omit it. Moor emits explicit `type: "stdio"`. Current installation entry points include Customize and Marketplace. Moor scans the user file; project configuration is imported by paste.
