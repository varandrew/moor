# moor

## 0.8.1

### 补丁更新

- 修复并核实 #66–#73，包含 main 已有的 Profile 工具治理与服务状态改进。
  - 洞察分位数每组只排序一次，减少重复计算；保留本次未合并的四次查询。
  - Profile 工具治理改为批量读取，保留禁用及未分配服务、禁用工具和名称冲突处理。
  - 审计日志时间统一为 UTC 固定纳秒精度，历史记录使用事务迁移，列表及洞察窗口统一为 `[from,to)`，非法或逆序时间参数返回 400。
  - 洞察页移除多余日志列表请求，刷新避免重复失效；工具筛选使用 300ms 防抖。
  - 时间窗口选择器复用紧凑 Tabs，补充保留策略及洞察边界回归测试。
  - 确认批量工具更新失败保留选择、成功重试清空选择；验证私网 MCP 请求路径，#73 缺少跨设备复现信息，保持 open。

## 0.8.0

### Minor Changes

- f5fd472: feat(clients): add Claude Desktop, Grok Build and Pi client presets
  - Client registry adds Claude Desktop (stdio bridged via `npx mcp-remote`), Grok Build (`~/.grok/config.toml`), and Pi (`~/.pi/agent/mcp.json`, requires the community `pi-mcp-adapter` package).
  - Claude Code preset renamed to `claude-code` with config path corrected to `~/.claude.json`.

- f5fd472: feat(logs): tool-call insights, governance panel and audit log retention
  - New insights endpoint aggregates tool-call usage by server and tool with error rates, surfaced in a governance view on the logs page.
  - Profile detail gains a tool governance panel with bulk server-state upsert and clone support.
  - An audit log retention sweeper now prunes old audit records automatically.

## 0.7.2

### Patch Changes

- c512eb8: feat(config): configuration import and client management enhancements
  - Client registry now declares a canonical gateway entry name; Kimi Code and dsh pin the entry to `moor-mcp`, so re-importing a client config replaces the handwritten entry instead of registering a second gateway connection.
  - ConverterPanel loads client snippets dynamically from the API instead of a hardcoded client list, and snippets now expose a stable `clientId`.
  - ServerCard and ServerDetail surface server status errors more robustly, and a mutation lock prevents concurrent execution of critical operations across hooks.
  - ConfigImportPanel disables the import button while an import is in progress.

- c512eb8: fix(types): export the `MCP_SESSION_IDLE_TTL_MS` constants from the `@moor/types` package entry (`DEFAULT` / `MIN` / `MAX`), so consumers no longer need to hardcode session idle TTL bounds.
- 0966c8a: Surface per-server log files for failed MCP server startups. Lifecycle events (start attempt, failure reason, unexpected exit) and redacted stderr (stdio servers only) are now written to `logs/<server-id>.log` under the app data directory, and startup error banners on the server card and detail page include an "Open Logs" button that opens the log file directly (with the full path shown on hover). Also extends credential redaction to URL userinfo (`scheme://user:pass@host`), which applies to both the log files and the existing startup error summaries.

## 0.7.1

### Patch Changes

- e0626d6: feat(gateway): MCP session idle TTL, capacity cap & SSE stream lifetime
  - Sessions now expire after an idle TTL (new setting `advanced.mcpSessionIdleTtlMs`, default 1h, valid range 5min–24h); validation on POST/GET refreshes liveness and a 60s background sweeper reclaims sessions leaked by crashed clients.
  - `initialize` beyond 128 concurrent sessions returns HTTP 503 instead of growing unboundedly.
  - GET SSE keep-alive streams close after a 30min total lifetime; clients reconnect per the Streamable HTTP spec.

  Closes #62

- f548056: Gateway: add Streamable HTTP GET SSE streams and MCP session handling (`Mcp-Session-Id`, DELETE) so Cursor and other Streamable HTTP clients can connect after `initialize` (thanks @835519608!)

## 0.7.0

### Minor Changes

- Gateway: add opt-in "Allow LAN MCP Access" advanced setting — binds 0.0.0.0 and accepts RFC1918 hosts on /mcp for WSL2 (NAT) and LAN clients; /api/\* stays loopback-only

### Patch Changes

- Gateway: fix Windows port fallback after quick restarts — bind with SO_REUSEADDR so the configured port survives TIME_WAIT, keeping MCP client configs stable

## 0.6.2

### Patch Changes

- Refactor audit logging and server management
  - Extract `AuditRecorder` to encapsulate audit logging (enabled check, active-profile resolution, redaction, persistence), replacing the standalone `record_audit` function
  - Add `ProfileService` domain service for profile persistence and typed domain events, slimming HTTP routes to transport concerns
  - Introduce `StdioHttpConnector` to manage connections per server config and simplify `ServerManager` connection logic
  - Make the `McpSession` trait async for tool management operations
  - Streamline `ServerService` creation and validation via the new `ServerInsertInput` struct
  - Adopt typed events on the event bus for clarity and maintainability

## 0.6.1

### Patch Changes

- Add a macOS setting to hide the Dock icon when closing to tray, improve stdio server PATH resolution by merging the login shell PATH, and update runtime discovery/API documentation.

## 0.6.0

### Minor Changes

- 0fd707c: Migrate from Node.js sidecar to unified Rust gateway
  - **Remove Node.js sidecar entirely**: Delete the `sidecar/` package including all TypeScript source, build scripts, tests, and configuration. Implements ADR-0001: single in-process Rust gateway.
  - **Complete Rust gateway features**: Add import service, config converter, import parser, audit redaction, and database migrations for the Rust implementation.
  - **Refactor HTTP error handling**: Introduce `AppError` for consistent error responses across all HTTP routes.
  - **Migrate to variable fonts**: Replace Google Fonts CDN with `@fontsource-variable` packages (Space Grotesk, Inter) for offline-first rendering and reduced external dependencies.
  - **Improve server lifecycle**: Handle mid-start interruptions gracefully, add session cleanup for stopped servers during startup phase.
  - **Update documentation**: Add CONTEXT.md, ADR-0001, update README translations (en, es, ja, zh) to reflect Rust-only architecture.

## 0.5.6

### Patch Changes

- Add configurable MCP request timeout and server startup timeout settings
  - Consolidate timeout settings into advanced config (`mcpRequestTimeoutMs`, `mcpServerStartTimeoutMs`)
  - Support dynamic timeout reading without server restart
  - Range: 5,000–300,000 ms, default 30,000 ms
  - Update Linux build dependencies and cache settings
  - Improve CI/CD specifications and installation documentation

## 0.5.5

### Patch Changes

- Add Linux build configuration for Tauri app (deb/rpm/AppImage)
- Standardize server update payloads with `ServerUpdateInput` type
- Introduce `ToolCategoryBadge` component for tool categorization
- Add `useEditSession` hook for managing server edit sessions with typed SSE events

## 0.5.4

### Patch Changes

- Fix inability to edit parameters and variables after adding an MCP server
  - Add duplicate key detection and visual feedback in key-value editors
  - Add unsaved changes confirmation when navigating away from server forms
  - Introduce `AlertDialog` and `UnsavedChangesDialog` components
- Improve SSE endpoint resolution with `{env:VAR}` placeholder support in HTTP headers
- Enhance Sonner Toast styling with rounded corners and better close button integration

## 0.5.3

### Patch Changes

- Refactor MCP client architecture: unify Stdio/HTTP transport abstraction, improve server lifecycle management and frontend state management

## 0.5.2

### Patch Changes

- Add Windows platform support and improve tool exposed name strategy
  - Add Windows x64 CI/CD build job producing installers
  - Adapt stdio environment handling for Windows (case-insensitive PATH, semicolon separator, PATHEXT resolution)
  - Conditionally compile tray icons per platform (macOS template icon / Windows regular icon)
  - Unify tool exposed names to `{serverSlug}__{toolName}` format, with shortest unique server ID prefix for slug collisions
  - Add `_meta.serverName` field to MCP gateway tools/list response
  - Switch home directory resolution to `dirs::home_dir()` for Windows compatibility

- Updated dependencies
  - @moor/types@0.5.2

## 0.5.1

### Patch Changes

- Refactor settings management to use database instead of file system, and enhance Rust toolchain setup in release workflow
- Updated dependencies
  - @moor/types@0.5.1

## 0.5.0

### Minor Changes

- Major rewrite introducing a Rust-native sidecar layer with full MCP communication support (stdio + Streamable HTTP + SSE transports), server lifecycle management with concurrency control, tool catalog discovery, settings persistence, database migrations, configuration import from popular MCP clients (Claude, Cursor, etc.), audit log redaction, and improved frontend hooks with abort signal support.

## 0.4.0

### Minor Changes

- Implement server ordering API and UI, enhance import API with improved candidate selection and error handling, refactor server management and tool catalog services, improve API error handling and validation responses, and add IPC patterns documentation with macOS login autostart improvements.

### Patch Changes

- Updated dependencies
  - @moor/types@0.4.0

## 0.3.0

### Minor Changes

- Moor v0.3.0 — Settings Center, SSE, React Query migration and full Sidecar refactor

  **Features:**
  - Settings Center with General / Appearance / Advanced groups (9 configurable items)
  - SSE auto-reconnection and real-time data streaming
  - React Query data layer migration (Dashboard, Profiles, Servers, ServerDetail, ProfileDetail)
  - stdio transport mode for MCP servers
  - Configuration converter for Claude Code, Codex, OpenCode and Cursor
  - Audit log service
  - Server lifecycle management with auto-start support
  - Session manager

  **Frontend:**
  - New Settings and AuditLogs pages
  - New shared components: ConverterPanel, CodeBlock, KeyValueTable, StatCard, DetailPageHeader
  - 6 new shadcn/ui components (Select, Checkbox, Textarea, Label, Separator, Skeleton)
  - SSE Context and useSettings / useTheme hooks
  - ServerCard and ScrollArea component refinements

  **Sidecar:**
  - Service layer extraction: Profiles, Settings, Import, Audit Log
  - Server Manager refactored to lifecycle-based architecture
  - API schemas layer with Zod v4 validation
  - DB layer enhancements (server-repository, tool-catalog)
  - Enhanced client config scanner with Cursor support

  **Tauri:**
  - Major Rust backend enhancements in lib.rs (286+ lines added)
  - Settings persistence and sidecar port management

  **Fixes:**
  - Node.js version requirement updated to 22+
  - Enhanced config import documentation
  - Unified error and warning messages to English

### Patch Changes

- Updated dependencies
  - @moor/types@0.3.0

## 0.3.0-beta.3

### Patch Changes

- feat: add auto-start for servers, React Query migration, and SSE reconnection

  **Features:**
  - Add auto-start functionality for servers with DB schema updates
  - Enhance SSE connection with automatic reconnection logic
  - Improve JSON parsing error handling in AddServerForm

  **Refactors:**
  - Migrate Dashboard, ProfileDetail, ServerDetail, and Servers pages to React Query
  - Introduce AddServerForm and ConfigImportPanel components
  - Remove unused hooks and simplify components
  - Unify profile and server interfaces to camelCase naming convention
  - Update all error and warning messages to English for consistency
  - Update test imports to use vite-plus/test and enhance test configurations

  **Fixes:**
  - Update moor version to 0.2.1-beta.1 in Cargo.lock

## 0.3.0-beta.2

### Minor Changes

- feat: add client configuration converter and new shadcn/ui components

  **Sidecar:**
  - Add configuration converter supporting Claude Code, Codex, OpenCode, and Cursor
  - Add `/api/import/convert` and `/api/import/parse` endpoints
  - Enhance scanner to support Cursor client configs
  - Add formatter functions for each client output format
  - Add sidecar build cache script for faster rebuilds
  - Refactor version sync scripts with core extraction and tests

  **Frontend:**
  - Add 6 new shadcn/ui components based on Radix UI: Select, Checkbox, Textarea, Label, Separator, Skeleton
  - Replace native `<select>`, `<input type="checkbox">`, and `<textarea>` elements with shadcn/ui equivalents
  - Add `ConverterPanel` component for cross-client MCP configuration conversion
  - Add `CodeBlock` shared component with copy-to-clipboard support
  - Fix `ServerCard` side-stripe border anti-pattern; use background tint for status indication
  - Unify icon button size system: `icon-sm` (32px) for dense UIs, `icon` (36px) standard
  - Enhance card close button visibility with 20px icons and stronger hover feedback
  - Enhance `ScrollArea` with Radix UI primitives and warm-toned scrollbar
  - Update `README.md` and `README.zh.md` with Radix UI and latest feature docs

## 0.2.1-beta.1

### Patch Changes

- feat: add stdio server management and legacy data migration
  - Support stdio command transport for MCP servers
  - Add legacy data directory migration for ~/.moor
  - Fix TypeScript resolution for node: prefixed modules

## 0.2.1-beta.0

### Patch Changes

- 584ab23: feat: add support for HTTP headers in server configuration and JSON import
  - Introduced `resolveHttpHeaders` function to resolve environment placeholders in HTTP headers.
  - Updated `ServerManager` to store and handle headers in server configurations.
  - Added `JsonImportEditor` component for importing and validating MCP JSON configurations.
  - Enhanced `Servers` page to include HTTP headers input in the server creation form.
  - Implemented JSON formatting and diagnostics for imported configurations.
  - Updated tests to cover new functionality related to headers and JSON import.

## 0.2.0

### Minor Changes

- 536ae72: Initial release of Moor - Local MCP Gateway Manager

### Patch Changes

- 536ae72: Harden release pipeline and fix changeset versioning
  - Unify CI runners on macos-latest with Rosetta 2 cross-compilation for x86_64,eliminating macos-13 queue bottlenecks
  - Fix GitHub Actions cache keys and add per-arch Node.js setup to ensure correctsidecar binary targets
  - Remove fixed version grouping between moor and moor-sidecar in changeset config
  - Refactor version-sync scripts to use sidecar/package.json as the source of truthand standardize JSON-based version writing
  - Auto-sync sidecar/CHANGELOG.md to repo root during release
  - Add CI/CD hardening spec documentation

- Harden release pipeline and fix changeset versioning
  - Unify CI runners on macos-latest with Rosetta 2 cross-compilation for x86_64, eliminating macos-13 queue bottlenecks
  - Fix GitHub Actions cache keys and add per-arch Node.js setup to ensure correct sidecar binary targets
  - Remove fixed version grouping between moor and moor-sidecar in changeset config
  - Refactor version-sync scripts to use sidecar/package.json as the source of truth and standardize JSON-based version writing
  - Auto-sync sidecar/CHANGELOG.md to repo root during release
  - Add CI/CD hardening spec documentation
