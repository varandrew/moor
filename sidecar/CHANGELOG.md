# moor-sidecar

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

- Add ServerUpdateInput type for standardized server updates, introduce ToolCategoryBadge component and useEditSession hook with typed SSE event handling, and add Linux build configuration for Tauri app.

## 0.5.4

### Patch Changes

- Enhance SSE endpoint resolution and add `{env:VAR}` placeholder support in HTTP headers (reads per-server env vars with process env fallback).

  Add duplicate key detection and unsaved changes confirmation to server forms.

  Introduce AlertDialog, KeyValueEditor (with duplicate key visual feedback), and UnsavedChangesDialog components.

  Improve Sonner Toast styling with rounded corners and better close button integration.

## 0.5.3

### Patch Changes

- d8c7d31: 重构 MCP 客户端架构，统一 Stdio/HTTP 传输层抽象，改进服务器生命周期管理和前端状态管理。

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
