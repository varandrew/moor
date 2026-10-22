<p align="center">
  <img src="src-tauri/icons/128x128.png" alt="Moor" width="128">
</p>

<h1 align="center">Moor</h1>

<p align="center">
  <b>AI Agent 向けローカル MCP ゲートウェイマネージャー</b><br>
  複数の MCP Server を単一エンドポイントに集約し、Profile によるツールフィルタリングを行い、美しいネイティブ UI ですべてを管理します。
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
  <a href="#install">インストール</a> ·
  <a href="#quickstart">クイックスタート</a> ·
  <a href="#features">機能</a> ·
  <a href="#architecture">アーキテクチャ</a> ·
  <a href="#development">開発</a> ·
  <a href="#api">API</a>
</p>

<!-- README-I18N:START -->

[English](./README.md) | [汉语](./README.zh.md) | **日本語** | [Español](./README.es.md)

<!-- README-I18N:END -->

---

> [!NOTE]
> この README は現在のソースツリーの機能を説明しています。配布版には、ここに記載された機能がまだ含まれていない場合があります。お使いのバージョンの[リリースノート](https://github.com/varandrew/moor/releases)をご確認ください。

> _AI Agent はツールを必要としますが、異なるクライアント間で多数の MCP Server を管理するのは面倒です。私はすべてを集約し、コンテキストでフィルタリングし、バックグラウンドで継続的に実行できる単一のゲートウェイが欲しかったのです——すべては美しいネイティブ UI から制御できます。_
>
> _Moor は単一エンドポイント（`http://127.0.0.1:<port>/mcp`）を公開し、アクティブな Profile に基づいて必要なツールのみを動的に提供します。Agent を切断することなく Profile を切り替えられ、ツール呼び出しの監査はデフォルトで有効です。これが私が Moor を作った理由です。_

<p align="center">
  <img src="./assets/Dashboard%20Page.png" alt="Dashboard" width="800"><br>
  <sub>Dashboard — アクティブな Profile、Server の状態、監査統計を一覧表示。</sub>
</p>

<p align="center">
  <img src="./assets/Servers%20Page.png" alt="Servers" width="800"><br>
  <sub>Servers — MCP Server の管理、設定のインポート、健全性の監視。</sub>
</p>

<p align="center">
  <img src="./assets/Profiles%20Page.png" alt="Profiles" width="800"><br>
  <sub>Profiles — Profile の作成、Server のオン/オフ、ツールの有効/無効化。</sub>
</p>

<p align="center">
  <img src="./assets/Audit%20Page.png" alt="Audit" width="800"><br>
  <sub>Audit — ツール呼び出しの全文脈とフィルタで詳細を確認。</sub>
</p>

<a id="install"></a>

## インストール

### macOS アプリ

[Releases](https://github.com/varandrew/moor/releases) から `.dmg` をダウンロードし、Applications フォルダにドラッグするだけです。デスクトッププロセス内に Rust HTTP ゲートウェイを組み込んでいるため、Node.js ランタイムの事前インストールは不要です。

### Windows アプリ

[Releases](https://github.com/varandrew/moor/releases) から Windows インストーラーをダウンロードして実行します。デスクトッププロセス内に Rust HTTP ゲートウェイを組み込んでいるため、Node.js ランタイムの事前インストールは不要です。

### Linux アプリ

[Releases](https://github.com/varandrew/moor/releases) から `.deb`、`.rpm`、または `.AppImage` をダウンロードします（CI で x86_64 と aarch64 向けにビルド）。デスクトッププロセス内に Rust HTTP ゲートウェイを組み込んでいるため、Node.js ランタイムの事前インストールは不要です。主な対象プラットフォームは引き続き macOS です。Linux 版は CI でビルドされますが、検証範囲は比較的限られています。

### ソースからビルド

macOS（Apple Silicon / Intel）、Windows x64、または Linux（x86_64 / aarch64）、Node.js >= 22（CI は 24）、pnpm 11.3.0（`package.json` で固定）、および CI で使用する Rust stable ツールチェーンが必要です。

```bash
git clone https://github.com/varandrew/moor.git
cd moor
pnpm install
```

ビルド手順については [開発](#development) セクションを参照してください。

<a id="quickstart"></a>

## クイックスタート

### アプリを起動する

アプリケーションメニューから **Moor**（macOS では `Moor.app`）を開きます。Dashboard では、アクティブな Profile、Server の状態、最近の監査ログが一目で確認できます。

### 既存設定をスキャンする

Moor は、Claude Code、Claude Desktop、Codex、OpenCode、Cursor、Kimi Code、dsh、Grok Build、Pi 用に既に設定済みの MCP Server を自動検出できます：

1. **Servers** → **Import** に移動
2. **Scan** をクリック — Moor は `~/.claude.json`、`~/.codex/config.toml`、`~/.config/opencode/opencode.json` / `.jsonc`、`~/.cursor/mcp.json`、`~/.kimi-code/mcp.json`、`~/.dsh/cordis.patch.yml`、`~/Library/Application Support/Claude/claude_desktop_config.json`、`~/.grok/config.toml`、`~/.pi/agent/mcp.json`（Pi は先に `pi install npm:pi-mcp-adapter` が必要）を読み取ります
3. インポートしたい Server を選択

**Import JSON** を使用して JSON MCP 設定を貼り付けることもできます。Moor は stdio および HTTP/SSE Server をインポートし、OpenAPI 設定などのサポートされていないエントリは保存せずに報告します。

インポートした Server を **Servers** ページで起動すると、Moor がツールを検出します。Profile 内の Server の切り替えはツールの公開を制御するもので、Server を起動する操作ではありません。

### プロファイルを作成する

Profile を使用すると、Server をグループ化し、Agent に公開するツールを制御できます：

1. **Profiles** → **New Profile** に移動
2. 名前を付ける（例："Coding"、"Research"）
3. Server のオン/オフを切り替える
4. Server を展開して個別のツールを有効/無効化
5. 必要に応じて **Curated listing** を選び、固定するツールを選択します。デフォルトは **Direct listing** です
6. **Activate** をクリック — 変更は即座に反映されます

### エージェントを接続する

MCP 互換の任意のクライアントを Moor の単一エンドポイントに向けます：

```
http://127.0.0.1:9223/mcp
```

`9223` はデフォルトのゲートウェイポートです。既に使用されている場合、Moor は次に利用可能なポートを選択し、Dashboard と Client Config ページに実際のエンドポイントを表示します。

デフォルトでは `/mcp` エンドポイントはループバックのみで、`X-Moor-Token` は不要です（LAN アクセスは下記を参照）。Moor は WebView とゲートウェイ間のローカル管理 API にのみ `X-Moor-Token` を使用するため、Agent の設定に貼り付ける必要はありません。

WSL2（NAT モード）や LAN 内の他のデバイスから接続するには、**Settings → Advanced → Allow LAN MCP Access** を有効にして Moor を再起動します。ゲートウェイはすべてのインターフェースでリッスンし、`/mcp` はプライベートネットワークのホスト（`10.x.x.x`、`172.16.x.x`–`172.31.x.x`、`192.168.x.x`）も受け付けます。`/api/*` は引き続きループバック限定で、トークンも必要です。Windows では初回にファイアウォールの許可が求められる場合があります。

Moor は残りのすべてを処理します — `tools/list` の集約、`tools/call` のルーティング、アクティブな Profile に基づくフィルタリング。

### MCP タイムアウトの調整

stdio Server を `npx`、`pnpm dlx`、または依存関係をダウンロードするパッケージマネージャー経由で起動すると、最初の `initialize` / `tools/list` がデフォルトの 30 秒を超える場合があります。**Settings → Advanced** で、起動時は **Server Start Timeout**、実行中の `tools/call` は **Request Timeout** を延長できます。どちらも 5–300 秒に設定できます。

よく使う Server を事前にインストールし、コマンドに絶対パスを指定すると起動待ち時間を減らせます。Moor の起動時に自動起動するには、各 Server の **Auto-start** と Settings の **Auto-start Servers on Launch** を両方有効にします。起動対象は、アクティブな Profile で有効になっている Server のみです。

<a id="features"></a>

## 機能

### MCP ゲートウェイの集約

単一の HTTP エンドポイント（`/mcp`）がすべてのバックエンド MCP Server をプロキシします。Agent は統一されたツールカタログを見ることができ — 複数のエンドポイントを設定する必要はありません。

### マルチトランスポート対応

**stdio**（サブプロセス）と **HTTP/SSE** の両方の MCP Server に接続できます。Moor は接続と Server のライフサイクルを管理し、必要に応じて定期的なヘルスチェックを有効にできます。

### プロファイル管理

異なるワークフロー向けに無制限の Profile を作成できます。各 Profile は以下を保存します：

- どの Server が有効か
- 各 Server でどのツールが無効か
- 固定したツール、公開モード、リスクポリシー
- グローバルアクティブ状態

Profile を **ホットスワップ** で切り替え — 接続中の Agent は接続を維持し、次の `tools/list` で新しい設定が即座に反映されます。

### ツール単位の切り替え

Server レベルのオン/オフに加えて、任意の Server を展開して特定のツールを無効化できます。無効化されたツールは Agent のツールカタログから除外され、呼び出すこともできません。

### ツールの選択的公開と固定

新規および既存の Profile は、デフォルトで **Direct listing** を使用します。このモードでは、有効かつ呼び出し可能で、Profile のポリシーが許可するすべてのツールを一覧に含めます。**Curated listing** では、固定したツールと次の 2 つのゲートウェイメタツールのみを一覧に含めます。

- `moor_search_tools`：キーワードと Server 名で利用可能なツールを検索します。ページ分割に対応し、結果には入力スキーマが含まれます
- `moor_call_tool`：検索結果の exposed name でツールを呼び出し、下流の実行結果を返します

Profile の固定操作で、個別のツールや Server 単位のツールを通常のカタログに残せます。Curated listing に切り替えると、最初に固定するツールの選択画面が開きます。固定ツールがなくても、カタログには 2 つのメタツールが残ります。固定していないツールも検索・呼び出しが可能で、キャッシュ済みの exposed name から直接呼び出すこともできます。無効化またはポリシーでブロックされたツールは呼び出せません。Direct listing にはいつでも戻せます。

### ツールのリスクポリシー

Moor はツールの名前と説明から、リスクを `safe`、`write`、`destructive`、`unknown` に分類します。Server の詳細ページで分類を手動修正できます。

各 Profile のデフォルトは **Advisory** で、リスクを表示するだけで呼び出しをブロックしません。**Block destructive** は `destructive` のツールを一覧と検索結果から除外し、エラーコード `-32002` と構造化された理由を返して呼び出しを拒否します。このポリシーは両方の公開モードで有効で、固定よりも優先されます。`write` や `unknown` のツールはブロックしません。分類はヒューリスティックによる推定であり、読み取り専用のアクセスを保証するものではありません。

### 設定のインポート

以下からワンクリックでインポート：

- **Claude Code**: `~/.claude.json`
- **Codex**: `~/.codex/config.toml`
- **OpenCode**: `~/.config/opencode/opencode.json` / `.jsonc`
- **Cursor**: `~/.cursor/mcp.json`
- **Kimi Code**: `~/.kimi-code/mcp.json`
- **dsh**: `~/.dsh/cordis.patch.yml`
- **Claude Desktop**: `~/Library/Application Support/Claude/claude_desktop_config.json`
- **Grok Build**: `~/.grok/config.toml`
- **Pi**: `~/.pi/agent/mcp.json`（先に `pi install npm:pi-mcp-adapter` が必要）

stdio および HTTP/SSE Server の手動入力と JSON 貼り付けによるバッチインポートもサポートされています。

### クライアント設定

Claude Code、Claude Desktop、Codex、OpenCode、Cursor、Kimi Code、dsh、Grok Build、Pi 向けにコピーするだけで使える設定スニペットを生成します。スニペットには `/mcp` エンドポイントのみが含まれます。Moor の `X-Moor-Token` は内部管理 API 用に予約されています。

### 監査ログ

監査ログが有効な場合（デフォルト）、ツール呼び出しには以下が記録されます：

- タイムスタンプ、Profile、Server、ツール名
- 引数（機密データはマスキング済み）
- 結果またはエラー
- 所要時間と Agent 情報
- 呼び出し元（`direct` または `meta`）と、ブロック時の拒否理由

時間範囲、Server、またはツールでフィルタリングできます。Dashboard には集計統計、Insights にはポリシーによるブロック件数が表示されます。監査は **Settings → Advanced** で無効にできます。ログの保存期間はデフォルトで 30 日です。`logRetentionDays` を `0` にすると無期限で保存します。

### システムトレイ

デフォルトでは、ウィンドウを閉じると Moor は macOS メニューバーまたは Windows システムトレイに隠れ、ゲートウェイは動作を続けます。Settings の **Minimize to Tray on Close** を無効にすると、ウィンドウを閉じたときに終了します。トレイメニューの **Quit Moor** でも終了できます。アプリを終了するとゲートウェイも停止します。

### リアルタイムステータス

Server の状態変更と Profile の切り替えは SSE を介して UI にプッシュされます。ページの更新は不要です。

`GET /mcp` を購読する MCP クライアントには、Profile、固定ツール、リスクポリシー、ツールカタログの変更時に `notifications/tools/list_changed` が届きます。この通知に対応するクライアントは、再接続せずに最新のカタログを取得できます。実行中の Server の詳細ページでツールを更新すると、プロセスを再起動せずにツールを再検出します。

<a id="architecture"></a>

## アーキテクチャ

<details>
<summary>アーキテクチャ図</summary>

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

### 通信フロー

```
AI Agent ──HTTP──▶ POST /mcp ──▶ Moor Gateway ──stdio/HTTP──▶ MCP Servers
                              │
WebView ──IPC──▶ get_sidecar_info ─┐
WebView ──env──▶ VITE_MOOR_API_URL/TOKEN ─┤ (ブラウザ開発モードでフォールバック)
WebView ──fetch──▶ /api/* ─────────┘
WebView ◀──SSE──── /api/events
```

- **ランタイム検出**: WebView → Tauri IPC (`get_sidecar_info`) → Rust（ポート、トークン）；ブラウザ開発モード（`pnpm dev`）では `VITE_MOOR_API_URL` / `VITE_MOOR_API_TOKEN` 環境変数にフォールバックします（`src/lib/api/runtime.ts` 参照。`GET /api/runtime` も存在しますがトークンが必要です）
- **業務操作**: WebView → HTTP `fetch()` → インプロセス Axum サーバー（Rust）
- **システム操作**: WebView → Tauri IPC → Rust（トレイ、ウィンドウ、自動起動）

<a id="development"></a>

## 開発

### 前提条件

- macOS（Apple Silicon / Intel）、Windows x64、または Linux（x86_64 / aarch64）
- [Node.js](https://nodejs.org) >= 22（CI は 24）
- [pnpm](https://pnpm.io) 11.3.0（`package.json` で固定）
- [Rust](https://rustup.rs) stable（CI で使用するツールチェーン）
- macOS では [Xcode Command Line Tools](https://developer.apple.com/xcode/resources/) も必要です

### 依存関係のインストール

```bash
pnpm install
```

### 開発モード

完全なデスクトップアプリ（Tauri、インプロセス Rust ゲートウェイ内蔵）を起動：

```bash
pnpm tauri dev
```

- フロントエンド: http://localhost:1420（WebView 内で Vite HMR 動作）

### プロダクションビルド

```bash
pnpm tauri build
```

出力：

- macOS: `src-tauri/target/release/bundle/macos/Moor.app`
- macOS DMG: `src-tauri/target/release/bundle/dmg/Moor_<version>_aarch64.dmg`
- Windows: `src-tauri/target/release/bundle/nsis/Moor_<version>_x64-setup.exe`
- Linux: `.deb` / `.rpm` / `.AppImage`（`src-tauri/target/release/bundle/` 配下、x86_64 + aarch64、CI でビルド）

### コード品質

```bash
vp check       # フォーマット + リント + 型チェック
vp lint        # リントのみ
vp lint --fix  # 自動修正
vp fmt         # フォーマット
```

### テスト

```bash
# Rust ゲートウェイテスト
cargo test --manifest-path src-tauri/Cargo.toml

# フロントエンド + スクリプトテスト
vp test
```

<a id="api"></a>

## API

`/api/*` の管理エンドポイントには、ループバックの Host と `X-Moor-Token` ヘッダーが必要です。`/mcp` は前述のループバック / LAN アクセス規則に従い、このトークンは不要です。

### MCP ゲートウェイ

| メソッド | パス   | 説明                                            |
| -------- | ------ | ----------------------------------------------- |
| `ALL`    | `/mcp` | MCP プロトコルエンドポイント（Streamable HTTP） |

### Server 管理

| メソッド | パス                                    | 説明                                                  |
| -------- | --------------------------------------- | ----------------------------------------------------- |
| `GET`    | `/api/servers`                          | すべての Server を一覧                                |
| `POST`   | `/api/servers`                          | Server を追加                                         |
| `GET`    | `/api/servers/:id`                      | Server 詳細                                           |
| `PUT`    | `/api/servers/:id`                      | Server 設定を更新                                     |
| `DELETE` | `/api/servers/:id`                      | Server を削除                                         |
| `POST`   | `/api/servers/:id/start`                | Server を起動                                         |
| `POST`   | `/api/servers/:id/stop`                 | Server を停止                                         |
| `GET`    | `/api/servers/:id/tools`                | 検出済みツールを取得（`?profile_id=` で絞り込み可能） |
| `POST`   | `/api/servers/:id/tools/refresh`        | 実行中の Server を再起動せずにツールを再検出          |
| `PATCH`  | `/api/servers/:id/tools/:toolName/risk` | ツールの `riskLevel` を手動で上書き                   |
| `PUT`    | `/api/servers/order`                    | Server の並べ替え                                     |

### プロファイル管理

| メソッド | パス                                  | 説明                                                                                         |
| -------- | ------------------------------------- | -------------------------------------------------------------------------------------------- |
| `GET`    | `/api/profiles`                       | すべての Profile を一覧                                                                      |
| `POST`   | `/api/profiles`                       | Profile を作成                                                                               |
| `GET`    | `/api/profiles/:id`                   | Profile 詳細（Server 含む）                                                                  |
| `PUT`    | `/api/profiles/:id`                   | 名前、`exposureMode`、`riskPolicy`、または Curated listing への移行時の `initialPins` を更新 |
| `DELETE` | `/api/profiles/:id`                   | Profile を削除                                                                               |
| `PUT`    | `/api/profiles/:id/activate`          | アクティブ Profile に設定                                                                    |
| `POST`   | `/api/profiles/:id/clone`             | Profile を複製                                                                               |
| `GET`    | `/api/profiles/:id/servers/:serverId` | Server の有効状態、無効ツール、固定ツールを取得                                              |
| `PUT`    | `/api/profiles/:id/servers/:serverId` | Server 切り替え + 無効ツールを更新                                                           |
| `PUT`    | `/api/profiles/:id/servers-state`     | Server の有効状態、`disabledTools`、`pinnedTools` を一括更新                                 |
| `GET`    | `/api/profiles/:id/tools`             | 公開状態、固定状態、リスク情報を含む Profile のツールグループを取得                          |

### 監査ログ

| メソッド | パス                 | 説明                                 |
| -------- | -------------------- | ------------------------------------ |
| `GET`    | `/api/logs`          | ログをクエリ（フィルタ対応）         |
| `GET`    | `/api/logs/stats`    | 集計統計                             |
| `GET`    | `/api/logs/insights` | ツール呼び出しのガバナンスインサイト |

### 設定管理

| メソッド | パス                  | 説明                 |
| -------- | --------------------- | -------------------- |
| `GET`    | `/api/settings`       | 設定を取得           |
| `PATCH`  | `/api/settings`       | 設定を更新           |
| `POST`   | `/api/settings/reset` | デフォルトにリセット |

### その他

| メソッド | パス                   | 説明                                   |
| -------- | ---------------------- | -------------------------------------- |
| `GET`    | `/api/health`          | ヘルスチェック                         |
| `GET`    | `/api/runtime`         | ランタイム情報                         |
| `GET`    | `/api/events`          | SSE リアルタイムイベントストリーム     |
| `POST`   | `/api/import/scan`     | ローカルクライアント設定をスキャン     |
| `POST`   | `/api/import/parse`    | 貼り付けた JSON インポートをプレビュー |
| `POST`   | `/api/import/execute`  | インポートを実行                       |
| `GET`    | `/api/import/snippets` | クライアント設定スニペットを生成       |
| `POST`   | `/api/import/convert`  | クライアント間で設定を変換             |

## 技術スタック

| レイヤー          | 技術                                                  |
| ----------------- | ----------------------------------------------------- |
| フロントエンド    | React 19, vite-plus, TypeScript 5.7, Tailwind CSS v4  |
| UI プリミティブ   | Radix UI                                              |
| UI コンポーネント | shadcn/ui (New York style)                            |
| デスクトップ      | Tauri 2 (Rust)                                        |
| ゲートウェイ      | Rust, Axum, Tokio, rusqlite (インプロセス)            |
| データベース      | SQLite (rusqlite)                                     |
| MCP プロトコル    | Rust 自前実装 (JSON-RPC over Streamable HTTP / stdio) |
| アイコン          | Lucide React                                          |
| ツールチェーン    | vite-plus (vp CLI), Oxlint, Oxfmt, Vitest             |

## 謝辞

[linuxdo](https://linux.do/) コミュニティの議論、共有、フィードバックに感謝します。

## ❤️ スポンサー

[![ko-fi](https://ko-fi.com/img/githubbutton_sm.svg)](https://ko-fi.com/varandrew)

## 🌟 Star 履歴

[![Star History Chart](https://api.star-history.com/svg?repos=varandrew/moor&type=Date)](https://www.star-history.com/#varandrew/moor&Date)

## ライセンス

[MIT](LICENSE)
