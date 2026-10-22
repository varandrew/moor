<p align="center">
  <img src="src-tauri/icons/128x128.png" alt="Moor" width="128">
</p>

<h1 align="center">Moor</h1>

<p align="center">
  <b>本地 MCP 网关管理器，为 AI Agent 提供统一的工具聚合与路由服务</b><br>
  将多个 MCP Server 聚合到单一端点，按 Profile 过滤工具，并通过精美的原生 UI 统一管理。
</p>

<p align="center">
  <img src="https://img.shields.io/badge/license-MIT-blue" alt="License">
  <img src="https://img.shields.io/badge/React-19-61DAFB?logo=react" alt="React 19">
  <img src="https://img.shields.io/badge/Tauri-2-24C8D8?logo=tauri" alt="Tauri 2">
  <img src="https://img.shields.io/badge/Node.js-22+-339933?logo=nodedotjs" alt="Node.js 22+">
  <img src="https://img.shields.io/badge/platform-macOS-black?logo=apple" alt="macOS">
  <img src="https://img.shields.io/badge/platform-Windows-0078D4?logo=windows" alt="Windows">
  <img src="https://img.shields.io/badge/platform-Linux-FCC624?logo=linux&logoColor=black" alt="Linux">
  <img src="https://img.shields.io/badge/pnpm-11+-F69220?logo=pnpm" alt="pnpm">
</p>

<p align="center">
  <a href="#安装">安装</a> ·
  <a href="#快速开始">快速开始</a> ·
  <a href="#功能特性">功能特性</a> ·
  <a href="#架构">架构</a> ·
  <a href="#开发">开发</a> ·
  <a href="#api">API</a>
</p>

<!-- README-I18N:START -->

[English](./README.md) | **汉语** | [日本語](./README.ja.md) | [Español](./README.es.md)

<!-- README-I18N:END -->

---

> [!NOTE]
> 本 README 描述当前源码中的功能。发行包可能尚未包含这里的全部功能，请以对应版本的[发布说明](https://github.com/varandrew/moor/releases)为准。

> _AI Agent 需要调用工具，但在不同客户端之间管理十几个 MCP Server 简直是一场灾难。我想要一个统一的网关，能够聚合所有工具、按场景过滤，并在后台持续运行——一切都可以通过一个精美的原生 UI 来掌控。_
>
> _Moor 暴露单一端点（`http://127.0.0.1:<port>/mcp`），根据当前激活的 Profile 动态只提供你需要的工具。切换 Profile 无需断开 Agent 连接，工具调用审计默认开启。这就是我打造 Moor 的原因。_

<p align="center">
  <img src="./assets/Dashboard%20Page.png" alt="Dashboard" width="800"><br>
  <sub>Dashboard — 总览当前 Profile、Server 状态与审计统计。</sub>
</p>

<p align="center">
  <img src="./assets/Servers%20Page.png" alt="Servers" width="800"><br>
  <sub>Servers — 管理 MCP Server、导入配置、监控健康状态。</sub>
</p>

<p align="center">
  <img src="./assets/Profiles%20Page.png" alt="Profiles" width="800"><br>
  <sub>Profiles — 创建 Profile、开关 Server、启用/禁用工具。</sub>
</p>

<p align="center">
  <img src="./assets/Audit%20Page.png" alt="Audit" width="800"><br>
  <sub>Audit — 查看每一次工具调用的完整上下文与筛选。</sub>
</p>

## 安装

### macOS 应用

从 [Releases](https://github.com/varandrew/moor/releases) 下载 `.dmg`，拖拽到 Applications 文件夹即可。桌面进程内嵌 Rust HTTP 网关——无需预装 Node.js 运行时。

### Windows 应用

从 [Releases](https://github.com/varandrew/moor/releases) 下载 Windows 安装包并运行即可。桌面进程内嵌 Rust HTTP 网关——无需预装 Node.js 运行时。

### Linux 应用

从 [Releases](https://github.com/varandrew/moor/releases) 下载 `.deb`、`.rpm` 或 `.AppImage`（CI 构建 x86_64 和 aarch64 版本）。桌面进程内嵌 Rust HTTP 网关——无需预装 Node.js 运行时。macOS 仍是主要目标平台；Linux 由 CI 构建，测试覆盖相对有限。

### 从源码构建

需要 macOS（Apple Silicon / Intel）、Windows x64 或 Linux（x86_64 / aarch64），以及 Node.js >= 22（CI 使用 24）、pnpm 11.3.0（固定于 `package.json`）和 CI 使用的 Rust stable 工具链。

```bash
git clone https://github.com/varandrew/moor.git
cd moor
pnpm install
```

构建说明请参阅[开发](#开发)章节。

## 快速开始

### 启动应用

从应用菜单打开 **Moor**（macOS 上为 `Moor.app`）。Dashboard 一览展示当前激活的 Profile、Server 运行状态和近期审计日志。

### 扫描现有配置

Moor 可以自动检测你已为 Claude Code、Claude Desktop、Codex、OpenCode、Cursor、Kimi Code、dsh、Grok Build 和 Pi 配置的 MCP Server：

1. 进入 **Servers** → **Import**
2. 点击 **Scan** — Moor 会读取 `~/.claude.json`、`~/.codex/config.toml`、`~/.config/opencode/opencode.json` / `.jsonc`、`~/.cursor/mcp.json`、`~/.kimi-code/mcp.json`、`~/.dsh/cordis.patch.yml`、`~/Library/Application Support/Claude/claude_desktop_config.json`、`~/.grok/config.toml` 以及 `~/.pi/agent/mcp.json`（Pi 需先安装社区 adapter：`pi install npm:pi-mcp-adapter`）
3. 选择要导入的 Server

你也可以通过 **Import JSON** 粘贴 JSON MCP 配置。Moor 支持导入 stdio 和 HTTP/SSE Server，对于不支持的条目（例如 OpenAPI 配置）会报告但不会保存。

在 **Servers** 页面启动导入的 Server，Moor 才能发现其工具。Profile 中的 Server 开关控制工具曝光，不会启动 Server。

### 创建 Profile

Profile 让你按场景分组 Server，并控制哪些工具暴露给 Agent：

1. 进入 **Profiles** → **New Profile**
2. 命名（例如 "Coding"、"Research"）
3. 开关 Server
4. 展开某个 Server 以启用/禁用单个工具
5. 可选：选择 **Curated listing** 并挑选置顶工具；默认使用 **Direct listing**
6. 点击 **Activate** — 切换即时生效

### 连接你的 Agent

将任意 MCP 兼容客户端指向 Moor 的单一端点：

```
http://127.0.0.1:9223/mcp
```

`9223` 是默认网关端口。如果该端口已被占用，Moor 会自动选择下一个可用端口，并在 Dashboard 和 Client Config 页面显示实际端点。

默认情况下 `/mcp` 端点仅限本地回环访问，不需要 `X-Moor-Token`（局域网访问见下文）。Moor 仅在 WebView 与网关之间的本地管理 API 中使用 `X-Moor-Token`，因此你无需将它粘贴到 Agent 配置中。

如需从 WSL2（NAT 模式）或局域网内其他设备连接，在 **Settings → Advanced** 开启 **Allow LAN MCP Access** 并重启 Moor。网关将监听所有网络接口，`/mcp` 额外接受私有网段主机（`10.x.x.x`、`172.16.x.x`–`172.31.x.x`、`192.168.x.x`）；`/api/*` 仍仅限回环并要求 Token。Windows 首次监听时可能弹出防火墙授权，需要允许。

Moor 会处理剩下的一切——聚合 `tools/list`、路由 `tools/call`、并根据激活的 Profile 进行过滤。

### 调整 MCP 超时

如果 stdio Server 通过 `npx`、`pnpm dlx` 或需要下载依赖的包管理器启动，首次 `initialize` / `tools/list` 可能超过默认 30 秒。进入 **Settings → Advanced**，调大 **Server Start Timeout** 可延长启动等待时间，调大 **Request Timeout** 可延长运行期 `tools/call` 等待时间。两项设置均支持 5–300 秒。

将常用 Server 预安装到本地并使用绝对命令路径，可减少启动等待。若要随 Moor 启动，需同时开启各 Server 的 **Auto-start** 和 Settings 中的 **Auto-start Servers on Launch**；仅启动当前激活 Profile 中已启用的 Server。

## 功能特性

### MCP 网关聚合

单一 HTTP 端点（`/mcp`）代理所有后端 MCP Server。Agent 看到的是统一的工具目录——无需配置多个端点。

### 多传输支持

同时支持 **stdio**（子进程）和 **HTTP/SSE** 两种 MCP Server 连接方式。Moor 管理连接与 Server 生命周期，并支持按需开启定期健康检查。

### Profile 管理

为不同工作流创建无限数量的 Profile。每个 Profile 记录：

- 启用了哪些 Server
- 每个 Server 禁用了哪些工具
- 置顶工具、曝光模式和风险策略
- 全局激活状态

支持**热切换**（Hot-Swap）——已连接的 Agent 保持连接，下次 `tools/list` 自动反映新配置。

### 工具级开关

除了 Server 级别的总开关，你还可以深入每个 Server，禁用特定工具。被禁用的工具不会出现在 Agent 的工具目录中，也不可调用。

### 策展曝光与置顶工具

新建及升级前已有的 Profile 默认使用 **Direct listing**，列出所有已启用、可调用且符合 Profile 策略的工具。**Curated listing** 仅列出置顶工具，以及两个网关元工具：

- `moor_search_tools`：按关键词和 Server 名称搜索可用工具，支持分页，结果包含输入参数 schema
- `moor_call_tool`：按搜索结果中的 exposed name 调用工具，返回下游执行结果

通过 Profile 的置顶控件，可将单个工具或某个 Server 的工具保留在默认目录中。切换到 Curated listing 时会打开初始置顶选择器。没有置顶工具时，目录仍包含两个元工具。未置顶工具仍可搜索和调用，也可使用此前缓存的 exposed name 直接调用；已禁用或被策略拦截的工具不可调用。可随时切回 Direct listing。

### 工具风险策略

Moor 根据工具名称和描述，将工具标记为 `safe`、`write`、`destructive` 或 `unknown`。可在 Server 详情页手动纠正分类。

每个 Profile 默认使用 **Advisory**，只显示风险、不拦截调用。**Block destructive** 会从目录和搜索结果中移除标记为 `destructive` 的工具，并以错误码 `-32002` 和结构化原因拒绝调用。策略在两种曝光模式下均生效，且优先于置顶。它不会拦截 `write` 或 `unknown` 工具；风险分类依赖启发式判断，不保证只读访问。

### 配置导入

一键导入来自以下客户端的配置：

- **Claude Code**: `~/.claude.json`
- **Codex**: `~/.codex/config.toml`
- **OpenCode**: `~/.config/opencode/opencode.json` / `.jsonc`
- **Cursor**: `~/.cursor/mcp.json`
- **Kimi Code**: `~/.kimi-code/mcp.json`
- **dsh**: `~/.dsh/cordis.patch.yml`
- **Claude Desktop**: `~/Library/Application Support/Claude/claude_desktop_config.json`
- **Grok Build**: `~/.grok/config.toml`
- **Pi**: `~/.pi/agent/mcp.json`（需先 `pi install npm:pi-mcp-adapter`）

也支持手动输入和粘贴 JSON 批量导入 stdio 与 HTTP/SSE Server。

### 客户端配置

为 Claude Code、Claude Desktop、Codex、OpenCode、Cursor、Kimi Code、dsh、Grok Build 和 Pi 生成即拷即用的配置片段。片段仅包含 `/mcp` 端点；Moor 的 `X-Moor-Token` 保留给内部管理 API 使用。

### 审计日志

审计开启时（默认开启），工具调用记录包含：

- 时间戳、Profile、Server、工具名
- 参数（敏感数据已脱敏）
- 结果或错误
- 耗时和 Agent 信息
- 调用来源（`direct` 或 `meta`）以及被拦截时的拒绝原因

支持按时间范围、Server 或工具筛选。Dashboard 提供聚合统计，Insights 展示策略拦截次数。可在 **Settings → Advanced** 关闭审计。日志默认保留 30 天；将 `logRetentionDays` 设为 `0` 可永久保留。

### 系统托盘

默认情况下，关闭窗口会将 Moor 隐藏到 macOS 菜单栏或 Windows 系统托盘，网关继续运行。关闭 Settings 中的 **Minimize to Tray on Close** 后，关窗即退出；也可通过托盘菜单 **Quit Moor** 退出。退出应用会停止网关。

### 实时状态

Server 状态变更和 Profile 切换通过 SSE 推送到 UI，无需刷新页面。

监听 `GET /mcp` 的 MCP 客户端会在 Profile、置顶、风险策略或工具目录变更时收到 `notifications/tools/list_changed`。处理该通知的客户端可重新获取目录，无需断开重连。在运行中 Server 的详情页刷新工具，可重新发现工具而不重启进程。

## 架构

<details>
<summary>架构图</summary>

```
Moor.app
├── UI Layer          React + Vite + TypeScript + Tailwind CSS v4 + shadcn/ui
├── Desktop Layer     Tauri 2 / Rust
│   ├── Window management + tray icon
│   └── In-process HTTP server (Axum)
│       ├── MCP protocol gateway   POST /mcp — init, tools/list, tools/call
│       ├── Server management      stdio spawn + HTTP/SSE client
│       ├── Profile routing        Global active Profile, hot-swap
│       ├── Audit logging          Tool call recording
│       └── SSE push               Real-time status sync to WebView
└── Storage           SQLite (rusqlite)
    ├── servers (configs, status)
    ├── profiles (server groups + tool toggles)
    └── audit_logs (tool calls, params, results, errors)
```

</details>

### 通信方式

```
AI Agent ──HTTP──▶ POST /mcp ──▶ Moor Gateway ──stdio/HTTP──▶ MCP Servers
                              │
WebView ──IPC──▶ get_sidecar_info ─┐
WebView ──env──▶ VITE_MOOR_API_URL/TOKEN ─┤ (纯浏览器开发模式回退)
WebView ──fetch──▶ /api/* ─────────┘
WebView ◀──SSE──── /api/events
```

- **运行时发现**: WebView → Tauri IPC (`get_sidecar_info`) → Rust（端口、token）；纯浏览器开发模式（`pnpm dev`）下回退到 `VITE_MOOR_API_URL` / `VITE_MOOR_API_TOKEN` 环境变量（见 `src/lib/api/runtime.ts`；`GET /api/runtime` 端点仍存在但需要 token）
- **业务操作**: WebView → HTTP `fetch()` → 进程内 Axum 服务器（Rust）
- **系统操作**: WebView → Tauri IPC → Rust（托盘、窗口、自启动）

## 开发

### 前置要求

- macOS（Apple Silicon / Intel）、Windows x64 或 Linux（x86_64 / aarch64）
- [Node.js](https://nodejs.org) >= 22（CI 使用 24）
- [pnpm](https://pnpm.io) 11.3.0（固定于 `package.json`）
- [Rust](https://rustup.rs) stable（CI 使用的工具链）
- macOS 还需要 [Xcode Command Line Tools](https://developer.apple.com/xcode/resources/)

### 安装依赖

```bash
pnpm install
```

### 开发模式

启动完整桌面应用（Tauri，内置进程内 Rust 网关）：

```bash
pnpm tauri dev
```

- 前端: http://localhost:1420（WebView 内置 Vite HMR）

### 生产构建

```bash
pnpm tauri build
```

输出：

- macOS: `src-tauri/target/release/bundle/macos/Moor.app`
- macOS DMG: `src-tauri/target/release/bundle/dmg/Moor_<version>_aarch64.dmg`
- Windows: `src-tauri/target/release/bundle/nsis/Moor_<version>_x64-setup.exe`
- Linux: `.deb` / `.rpm` / `.AppImage`，位于 `src-tauri/target/release/bundle/`（x86_64 + aarch64，由 CI 构建）

### 代码质量

```bash
vp check       # 格式化 + 检查 + 类型检查
vp lint        # 仅检查
vp lint --fix  # 自动修复
vp fmt         # 格式化
```

### 测试

```bash
# Rust 网关测试
cargo test --manifest-path src-tauri/Cargo.toml

# 前端 + 脚本测试
vp test
```

## API

`/api/*` 管理接口要求回环 Host 和 `X-Moor-Token` 请求头。`/mcp` 遵循前述回环 / LAN 访问规则，不需要该 Token。

### MCP 网关

| 方法  | 路径   | 说明                            |
| ----- | ------ | ------------------------------- |
| `ALL` | `/mcp` | MCP 协议端点（Streamable HTTP） |

### Server 管理

| 方法     | 路径                                    | 说明                                         |
| -------- | --------------------------------------- | -------------------------------------------- |
| `GET`    | `/api/servers`                          | 列出所有 Server                              |
| `POST`   | `/api/servers`                          | 添加 Server                                  |
| `GET`    | `/api/servers/:id`                      | Server 详情                                  |
| `PUT`    | `/api/servers/:id`                      | 更新 Server 配置                             |
| `DELETE` | `/api/servers/:id`                      | 删除 Server                                  |
| `POST`   | `/api/servers/:id/start`                | 启动 Server                                  |
| `POST`   | `/api/servers/:id/stop`                 | 停止 Server                                  |
| `GET`    | `/api/servers/:id/tools`                | 获取已发现工具（可通过 `?profile_id=` 筛选） |
| `POST`   | `/api/servers/:id/tools/refresh`        | 重新发现运行中 Server 的工具，不重启进程     |
| `PATCH`  | `/api/servers/:id/tools/:toolName/risk` | 手动覆盖工具的 `riskLevel`                   |
| `PUT`    | `/api/servers/order`                    | 重新排序 Server                              |

### Profile 管理

| 方法     | 路径                                  | 说明                                                                     |
| -------- | ------------------------------------- | ------------------------------------------------------------------------ |
| `GET`    | `/api/profiles`                       | 列出所有 Profile                                                         |
| `POST`   | `/api/profiles`                       | 创建 Profile                                                             |
| `GET`    | `/api/profiles/:id`                   | 获取 Profile 详情（含 Servers）                                          |
| `PUT`    | `/api/profiles/:id`                   | 更新名称、`exposureMode`、`riskPolicy`，或进入策展模式时的 `initialPins` |
| `DELETE` | `/api/profiles/:id`                   | 删除 Profile                                                             |
| `PUT`    | `/api/profiles/:id/activate`          | 激活 Profile                                                             |
| `POST`   | `/api/profiles/:id/clone`             | 克隆 Profile                                                             |
| `GET`    | `/api/profiles/:id/servers/:serverId` | 获取 Server 开关、禁用工具和置顶工具                                     |
| `PUT`    | `/api/profiles/:id/servers/:serverId` | 更新 Server 开关 + 禁用工具                                              |
| `PUT`    | `/api/profiles/:id/servers-state`     | 批量更新 Server 开关、`disabledTools` 和 `pinnedTools`                   |
| `GET`    | `/api/profiles/:id/tools`             | 获取 Profile 工具分组及可见性、置顶和风险信息                            |

### 审计日志

| 方法  | 路径                 | 说明                 |
| ----- | -------------------- | -------------------- |
| `GET` | `/api/logs`          | 查询日志（支持筛选） |
| `GET` | `/api/logs/stats`    | 聚合统计             |
| `GET` | `/api/logs/insights` | 工具调用治理洞察     |

### Settings 管理

| 方法    | 路径                  | 说明         |
| ------- | --------------------- | ------------ |
| `GET`   | `/api/settings`       | 获取设置     |
| `PATCH` | `/api/settings`       | 更新设置     |
| `POST`  | `/api/settings/reset` | 重置为默认值 |

### 其他

| 方法   | 路径                   | 说明                 |
| ------ | ---------------------- | -------------------- |
| `GET`  | `/api/health`          | 健康检查             |
| `GET`  | `/api/runtime`         | 运行时信息           |
| `GET`  | `/api/events`          | SSE 实时事件流       |
| `POST` | `/api/import/scan`     | 扫描本地客户端配置   |
| `POST` | `/api/import/parse`    | 预览粘贴的 JSON 导入 |
| `POST` | `/api/import/execute`  | 执行导入             |
| `GET`  | `/api/import/snippets` | 生成客户端配置片段   |
| `POST` | `/api/import/convert`  | 客户端配置互转       |

## 技术栈

| 层级     | 技术                                                 |
| -------- | ---------------------------------------------------- |
| 前端     | React 19, vite-plus, TypeScript 5.7, Tailwind CSS v4 |
| UI 基础  | Radix UI                                             |
| UI 组件  | shadcn/ui (New York style)                           |
| 桌面框架 | Tauri 2 (Rust)                                       |
| 网关     | Rust, Axum, Tokio, rusqlite (进程内)                 |
| 数据库   | SQLite (rusqlite)                                    |
| MCP 协议 | Rust 自实现 (JSON-RPC over Streamable HTTP / stdio)  |
| 图标     | Lucide React                                         |
| 工具链   | vite-plus (vp CLI), Oxlint, Oxfmt, Vitest            |

## 鸣谢

感谢 [linuxdo](https://linux.do/) 社区的讨论、分享与反馈。

## ❤️ 赞助

[![ko-fi](https://ko-fi.com/img/githubbutton_sm.svg)](https://ko-fi.com/varandrew)

## 🌟 Star 历史

[![Star History Chart](https://api.star-history.com/svg?repos=varandrew/moor&type=Date)](https://www.star-history.com/#varandrew/moor&Date)

## 许可证

[MIT](LICENSE)
