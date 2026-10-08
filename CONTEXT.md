# Moor

Local MCP Gateway Manager: aggregates multiple MCP servers behind one HTTP endpoint, filters tools by Profile, and audits every call. The business layer is a single Rust in-process gateway (`src-tauri/src/sidecar/`); the legacy Node sidecar was removed (ADR-0001).

## Language

### Aggregation & routing

**Gateway**:
The Smart Aggregator that exposes one `/mcp` endpoint, proxies `tools/list` and `tools/call`, and filters by the Active Profile.
_Avoid_: proxy, server (it is both an MCP server and client).

**Active Profile**:
The single Profile in effect for all Agents at a time; its server/tool toggles decide what the Gateway exposes.
_Avoid_: current profile, selected profile.

**Hot-Swap**:
Changing the Active Profile without disconnecting Agents; the next `tools/list` reflects the change. Affects exposed Tools only, never a Server's run status. Activation emits `profile:activated`; the frontend invalidates its caches on it (contract test: `profile_service` tests).
_Avoid_: reload, switch.

**exposed name**:
The name a Tool is published under to Agents, derived from the server slug and `tool_name`, with a short server-id suffix on collision. Distinct from `tool_name` (the backend MCP server's own name).
_Avoid_: alias, public name.

### Catalog

**Server**:
A configured backend MCP server (stdio or http), with config and a run status (`stopped | starting | running | error`).
_Avoid_: connection, MCP (reserve "MCP" for the protocol).

**Profile**:
A named set of Servers plus per-server enable + per-tool deny list. Exactly one is active.
_Avoid_: workspace, group.

**Tool / ToolDiscovery**:
A Tool is a callable an MCP Server offers. A ToolDiscovery is the cached row (`tool_name`, `exposed_name`, schema) discovered from a Server.
_Avoid_: capability, function.

**ProfileServer**:
The join of a Profile and Server carrying `enabled` and the `disabled_tools` deny list.
_Avoid_: membership, assignment.

### Import & observability

**Config Import**:
Reading MCP server configs from an external client (Scan), pasted text (preview), executing the import, and converting Moor's servers back out to a client format.
_Avoid_: sync, migration.

**Client Dialect**:
The per-client body of Config Import knowledge — registry entry (id, paths, format, gateway entry name), JSON entry shapes, and env-ref syntax. Single authority: Rust `clients.rs` registry + `formatters.rs` dialect table; the frontend client list derives from `/api/import/snippets` (`clientId`).
_Avoid_: client list, client config map (the scattered copies).

**Scan**:
Detecting servers from a known client's config file (registry-driven — see `clients.rs` `ALL_CLIENTS`; currently 9 clients).
_Avoid_: discover (reserve for ToolDiscovery).

**Audit Log**:
The record of every `tools/call`: profile, server, tool, redacted arguments, result/error, duration, agent.
_Avoid_: history, trace.

## Modules (architecture)

Names for deepened modules from the 2026-05-29 architecture review, updated for the single-Rust decision (ADR-0001).

**Server Runtime**:
The one deep module owning a Server's full lifecycle — registry, status state machine, sessions, and tool catalog. Canonical implementation: Rust `server_manager.rs` (the aggregate root) with in-domain files `server_manager/{status,session,errors}.rs`; the 14-method public interface is unchanged. The Node `server-manager` / `server-service` / `server-lifecycle` / `session-manager` split was removed, not collapsed.
_Avoid_: ServerManager, ServerService (the split).

**Config Import**:
The deep module covering scan / preview / execute / convert. Deepened 2026-08-18: the Client Dialect seam — `clients.rs` registry (single client list, `gateway_entry_name`) + `formatters.rs` `json_dialect` table (entry shapes) + loud `no formatter` error (guard test `every_registered_client_has_a_formatter`). The frontend derives its client list from snippets, no hardcoded copies.
_Avoid_: import pipeline, the loose parts.

**Server Status**:
The pure reducer (frontend) that resolves a Server's displayed status from base query data, optimistic action, SSE `server:status` event, and mutation settle, and derives every cache transition (list + detail channels): start/stop optimistic & failed patches, add/remove list transitions, and the shared SSE merge rule. Canonical implementation: `src/lib/server-status.ts`. The interface is its test surface.
_Avoid_: server patch utils, status merge, inline cache writes in hooks.

**Profile Tool Governance**:
管理 Profile 内 Tool 的启禁状态，统一单项、整组和批量操作。同一 Profile 的治理写入按序执行；Undo 恢复操作前的禁用清单，仅在该 Profile 后续写入尚未成功时有效。未发现的 Tool 仍保留其既有禁用状态。
_Avoid_: 工具快照工具集、页面内禁用清单计算。

**Server Diagnostics**:
Server Runtime 内按启动尝试归属的诊断记录。统一对生命周期与 stderr 内容脱敏，诊断失败不影响 Server 启动；新启动尝试开始后，旧尝试不得继续写入当前日志。
_Avoid_: 裸日志路径、各调用方自行降级。
