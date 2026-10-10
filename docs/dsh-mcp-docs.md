# DeepSeek Harness (dsh) MCP Reference

> Source: https://github.com/deepseek-ai/deepseek-harness/blob/master/packages/mcp/mcp-client/README.md
> Checked: 2026-10-08
> Applicable version: Current official documentation; version-specific requirements are noted below.
> Additional sources: https://github.com/deepseek-ai/deepseek-harness/blob/master/packages/mcp/README.md, https://github.com/deepseek-ai/deepseek-harness/blob/master/examples/mcp-memory/mcp-reference-memory.cordis.yml, https://github.com/deepseek-ai/deepseek-harness/blob/master/README.md, https://www.deepseek.com/harness/
>
> Note: This document is an edited, user-oriented excerpt of the sources above (the primary source is a package-level, developer-facing README), not a verbatim mirror. Copyright belongs to the original site; content may be outdated, please refer to the official links. Follow the original site's license when citing or redistributing.

## MCP in dsh (plugin-based)

DeepSeek Harness (`dsh`) is an open-source agent harness developed by DeepSeek AI, built on the Cordis plugin system with an "everything is a plugin" architecture: models, tools, skills, sessions, sandbox, storage, loops, scheduling, and UI are all provided by plugins. MCP support follows the same model — the `@deepseek-ai/dsh-mcp-client` plugin bridges the harness to external [Model Context Protocol](https://modelcontextprotocol.io/) servers: it connects to a server and registers that server's tools on `ctx.tools`, so the model sees them as native tools under server-qualified names (`mcp__<serverName>__<rawName>`) — the same shape Claude Code and Codex use.

dsh is currently in _developer preview_ and iterating rapidly. **There will be compatibility-breaking changes.**

## Configuring MCP servers (cordis.patch.yml)

MCP servers are configured as Cordis plugin rows — **one plugin instance per MCP server** in the user patch `cordis.patch.yml`:

```yaml
- insert:
    - id: mcp-github
      name: "@deepseek-ai/dsh-mcp-client"
      config:
        serverName: github
        transport: stdio
        command: npx
        args: ["-y", "@modelcontextprotocol/server-github"]
        env:
          GITHUB_TOKEN: !!js process.env.GITHUB_TOKEN

    - id: mcp-web
      name: "@deepseek-ai/dsh-mcp-client"
      config:
        serverName: web
        transport: streamable-http
        url: http://localhost:3000/mcp
        headers:
          Authorization: !!js "`Bearer ${process.env.MCP_TOKEN}`"
```

Notes on configuration:

- There is no dedicated user-level/project-level MCP scope as in other clients — MCP servers live at the plugin layer of the active profile's configuration. `dsh plugin --profile <name> add <pkg>` installs a plugin into a profile (forwards to pnpm); `dsh --profile <name> --dump-config` prints the composed config tree without booting.
- `env` values (and other YAML values) support the `!!js` extension to inject expressions evaluated against the Node runtime, e.g. `!!js process.env.GITHUB_TOKEN`.
- dsh spawns the configured `command` but does not run a package manager. Per the official example, install the pinned executable first (`command: mcp-server-memory` below assumes a pre-installed binary); a runner such as `npx` also works as the command.

A real-world example from the repository (`examples/mcp-memory/mcp-reference-memory.cordis.yml`):

```yaml
- insert:
    - id: memory-mcp-reference
      name: "@deepseek-ai/dsh-mcp-client"
      config:
        serverName: reference_memory
        transport: stdio
        command: mcp-server-memory
        cwd: !!js process.cwd()
        env:
          MEMORY_FILE_PATH: !!js >-
            process.env.MEMORY_FILE_PATH?.trim() || process.getBuiltinModule('node:path').join(process.getBuiltinModule('node:os').homedir(), '.dsh-mcp-reference-memory.jsonl')
```

## Configuration fields

| Field                      | Transport | Required | Description                                                                                              |
| -------------------------- | --------- | -------- | -------------------------------------------------------------------------------------------------------- |
| `transport`                | both      | yes      | `"stdio"` or `"streamable-http"`                                                                         |
| `serverName`               | both      | yes      | Namespace for this server's model-facing tool names; `[A-Za-z0-9_-]{1,32}`, unique across live instances |
| `command`                  | stdio     | yes      | Executable to spawn                                                                                      |
| `args`                     | stdio     | no       | Arguments passed to the command                                                                          |
| `env`                      | stdio     | no       | Extra env vars merged on top of scrubbed ambient env                                                     |
| `cwd`                      | stdio     | no       | Working directory for the child process                                                                  |
| `url`                      | http      | yes      | MCP server URL                                                                                           |
| `headers`                  | http      | no       | Extra headers (e.g. auth tokens)                                                                         |
| `toolCallTimeoutMs`        | both      | no       | Timeout per `callTool` invocation (default 60000)                                                        |
| `failOnStartupError`       | both      | no       | Reject plugin activation when initial connection or tool synchronization fails (default `false`)         |
| `reconnect.enabled`        | both      | no       | Reconnect automatically after a lost connection (default `true`)                                         |
| `reconnect.initialDelayMs` | both      | no       | First reconnect delay in ms; doubles per consecutive failed attempt (default 500)                        |
| `reconnect.maxDelayMs`     | both      | no       | Backoff ceiling in ms; also the uptime after which the attempt budget resets (default 30000)             |
| `reconnect.maxAttempts`    | both      | no       | Consecutive failed attempts per outage before giving up for good (default 10)                            |

## Supported transports

Two transports are supported: **stdio** (local child process) and **streamable-http** (remote HTTP endpoint). SSE transport is not supported. For HTTP servers, authentication is configured through the `headers` field (e.g. bearer tokens); no OAuth flow is documented for this plugin.

## Tool naming

Every MCP tool has two names: the raw MCP name (sent on the wire in `tools/call`) and the public name `mcp__<serverName>__<rawName>` registered on `ctx.tools`. Public names are normalized to the DeepSeek function-name contract (64 chars, `[A-Za-z0-9_-]`); when replacement or truncation changes the name, a deterministic 12-hex-char hash of `(serverName, rawName)` is appended so distinct tools never collapse into one name. Names are pure functions of `(serverName, rawName)` — connection order, re-syncs, and other servers never rename a tool.

- Two servers publishing the same raw name (e.g. `search`) coexist under their namespaces.
- A duplicate `serverName` across live instances fails the later plugin instance at load.
- A server listing the same tool name twice is rejected as an invalid tool list.

## Connection and reconnection behavior

- On connect: plugin activation awaits `listTools()` and registers every tool before the agent starts its first turn. Initial connection, discovery, or registration failure is always logged; it rejects activation when `failOnStartupError` is `true`, and otherwise activates with no tools.
- Tool list changes: the plugin listens for `notifications/tools/list_changed` and re-syncs. A fetch-phase failure keeps the previous generation registered; a registration conflict rolls back the attempted generation entirely, leaving no tools from that server.
- On disconnect/crash: the supervisor restarts the original server config with exponential backoff (`reconnect.initialDelayMs` doubling up to `reconnect.maxDelayMs`) and re-runs discovery on success. During the outage, the last good generation stays registered; calls against it fail until recovery.
- Reconnection is budgeted per outage: after `reconnect.maxAttempts` consecutive failures the server's tools are unregistered and reconnection stops until an HMR reload or host restart. A connection that survives past `maxDelayMs` resets the budget, so an occasionally-crashing server recovers indefinitely while a crash-looping one still exhausts the cap.
- With `reconnect.enabled: false`, a lost connection keeps tools registered but failing until a reload (manual-recovery behavior).
- Reconnect states are user-visible in logs: reconnecting (warn, with attempt count and delay), recovered (info), final failure and disabled-loss (error).
- HMR hot-swap: editing the plugin entry triggers disconnect + reconnect without a process restart; an unchanged `serverName` reproduces identical tool names.

## Known limitations

- **Resources** are read on demand; Prompts remain unsupported.
- **Startup timeout is inherited from the MCP SDK** — dsh does not yet expose a connection/discovery timeout. Each initialize or paginated `tools/list` request uses the SDK's 60-second default, so an unresponsive server can delay activation and teardown.
- **Reconnect triggers on transport close** — a crashed stdio child fires it; Streamable HTTP failures surface per request and through the SDK transport's own SSE-stream recovery, so an unreachable HTTP server is retried per call rather than respawned by the supervisor.
- **Images** can enter model context after capability verification; other non-text support depends on the harness.
- **Output validation** is performed by the MCP SDK; unsupported model representations still require harness-specific handling.

## Current configuration notes

User patch: `$DSH_HOME/cordis.patch.yml` (default `~/.dsh/cordis.patch.yml`). Profile patch: `$DSH_HOME/profiles/<name>/cordis.patch.yml`. Append plugin rows under `- insert:`. Disable a Cordis row using its row-level `disabled` field, not `config.enabled`. `maxInstructionBytes` defaults to 32768.

Moor supports DSH generation/export only. YAML scanning and paste import are unsupported, and dynamic `!!js` expressions are never evaluated.
