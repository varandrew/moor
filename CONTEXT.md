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
变更 Active Profile 而不主动断开 Agent；后续 `tools/list` 和新接受的 `tools/call` 使用新 Profile。已解析的调用保留原 Tool Call Context。一次 Tool Exposure 读取固定 Profile、曝光模式、Tool Policy 和 Tool 行；Hot-Swap 不将已开始的读取拼成新旧 Profile 混合结果。只影响暴露的 Tool，不改变 Server 的运行状态。激活时发送 `profile:activated`，前端据此使缓存失效。
_Avoid_: reload, switch.

**Tool Call Context**:
一次 Tool 调用解析时固定的 Profile、Server 与原始 `tool_name`。执行和 Audit Log 共用该归属，不在完成时重新读取 Active Profile；日志中的 Tool 名称保留 Agent 使用的 exposed name。固定归属不改变 Server 现有的停止与重启规则。
_Avoid_: 完成时的活动方案、重复解析 Tool 归属。

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

### Curation & policy

**Tool Exposure**:
网关向 Agent 发布 Tool 的方式。两种模式：Direct（Profile 内全部启用的 Tool 直出）与 Curated（置顶 Tool 直出，其余按需发现）。一个 Profile 同时只有一种模式。读取存储失败时明确返回错误，只有确实没有 Active Profile 时才返回空清单；空的 Curated Profile 仍保留其元工具。
_Avoid_: 工具过滤、白名单模式、懒加载。

**Curated Exposure**:
Tool Exposure 的策展模式：Profile 置顶的 Tool 直接出现在 `tools/list`，其余 Tool 经网关的检索与转发元工具发现和调用；两条路径共用同一套 Tool Call Context 与 Audit Log。进入 Curated Exposure 时，追加所选 Pinned Tool 与切换模式一同提交；任一步失败均保留原状态。空选择不清空已有置顶。
_Avoid_: tool search mode、动态工具。

**Pinned Tool**:
在 Profile 内标记为常驻直出的 Tool。未置顶的 Tool 仅在 Curated Exposure 下按需可见。
_Avoid_: 常用工具、收藏。

**Tool Policy**:
Profile 级的 Tool 准入规则，在 `tools/list`（过滤）与 `tools/call`（结构化拒绝）两层同时生效，并记录拒绝原因。deny 优先于 allow。
_Avoid_: 权限、安全策略（过于宽泛）。

**Moorfile**:
可导出的单机 Profile 包，描述 Profile、Server 与 Tool Policy，密钥以占位符表示而不含真实值。
_Avoid_: 备份、导出文件。

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
审计开启时记录 `tools/call` 的 Profile、Server、Tool、脱敏参数、结果或错误、耗时与 Agent。使用 Tool Call Context 中的归属；写入时关联已删除则仅将该关联设为 NULL，保留记录。审计失败不影响调用结果。
_Avoid_: history, trace.

## Modules (architecture)

Names for deepened modules from the 2026-05-29 architecture review, updated for the single-Rust decision (ADR-0001).

**Server Runtime**:
管理 Server 完整生命周期的 deep module，统一 registry、状态机、会话、Tool 清单、Tool 调用与审计归属。规范实现为 Rust `server_manager.rs` 及域内文件 `server_manager/{status,session,errors,health}.rs`。`call_tool(exposed_name, arguments, agent_info)` interface 一次解析 Tool Call Context，完成执行、计时与审计，区分 Tool 不可用和执行失败；MCP 协议层只负责参数校验与响应编码。ToolDiscovery 仅在仍属于当前启动代次且保存成功时被接纳并发布变更；启动时保存失败须断开新会话并报告启动失败。手动刷新保存失败保留原清单与运行状态，停止或重启导致的过期结果须明确报告失效。
_Avoid_: ServerManager, ServerService (the split).

**Settings**:
管理设置读取、更新与重置的 deep module，规范实现为 `src/hooks/useSettings.ts`。`updateSettings(payload)` 与 `resetSettings()` interface 在同一 QueryClient 内串行执行；涉及登录自启时读取执行前已保存的值作为补偿基线。保存失败则恢复登录自启，补偿失败保留两种错误；保存成功立即更新缓存，运行时应用失败明确报告部分成功，不撤销已保存设置。页面负责交互确认与状态展示，语言上下文负责语言同步。
_Avoid_: 页面内登录自启补偿、调用方自行同步运行时。

**Config Import**:
The deep module covering scan / preview / execute / convert. Deepened 2026-08-18: the Client Dialect seam — `clients.rs` registry (single client list, `gateway_entry_name`) + `formatters.rs` `json_dialect` table (entry shapes) + loud `no formatter` error (guard test `every_registered_client_has_a_formatter`). The frontend derives its client list from snippets, no hardcoded copies.
_Avoid_: import pipeline, the loose parts.

**Server Status**:
The pure reducer (frontend) that resolves a Server's displayed status from base query data, optimistic action, SSE `server:status` event, and mutation settle, and derives every cache transition (list + detail channels): start/stop optimistic & failed patches, add/remove list transitions, and the shared SSE merge rule. Canonical implementation: `src/lib/server-status.ts`. The interface is its test surface.
_Avoid_: server patch utils, status merge, inline cache writes in hooks.

**Profile Tool Governance**:
管理 Profile 内 Tool 的启禁状态，统一单项、整组和批量操作。同一 Profile 的启禁、置顶、曝光模式与风险策略写入按序执行；切换失败保留界面选择，成功后使旧 Undo 失效。模式切换不提供 Undo。启禁与置顶的 Undo 恢复操作前的相应清单，仅在该 Profile 后续写入尚未成功时有效。未发现的 Tool 仍保留其既有禁用状态。
_Avoid_: 工具快照工具集、页面内禁用清单计算。

**Server Diagnostics**:
Server Runtime 内按启动尝试归属的诊断记录。统一对生命周期与 stderr 内容脱敏，诊断失败不影响 Server 启动；新启动尝试开始后，旧尝试不得继续写入当前日志。
_Avoid_: 裸日志路径、各调用方自行降级。
