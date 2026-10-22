<p align="center">
  <img src="src-tauri/icons/128x128.png" alt="Moor" width="128">
</p>

<h1 align="center">Moor</h1>

<p align="center">
  <b>Administrador de Gateway MCP local para Agentes de IA</b><br>
  Agrega múltiples servidores MCP en un solo endpoint, filtra herramientas por Perfil y gestiona todo desde una hermosa UI nativa.
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
  <a href="#install">Instalación</a> ·
  <a href="#quickstart">Inicio rápido</a> ·
  <a href="#features">Características</a> ·
  <a href="#architecture">Arquitectura</a> ·
  <a href="#development">Desarrollo</a> ·
  <a href="#api">API</a>
</p>

<!-- README-I18N:START -->

[English](./README.md) | [汉语](./README.zh.md) | [日本語](./README.ja.md) | **Español**

<!-- README-I18N:END -->

---

> [!NOTE]
> Este README describe el árbol de código fuente actual. Las versiones distribuidas pueden no incluir todavía todas las funciones documentadas aquí; consulta las [notas de la versión](https://github.com/varandrew/moor/releases) que utilizas.

> _Los Agentes de IA necesitan herramientas, pero gestionar docenas de servidores MCP entre diferentes clientes es un desastre. Quería una única puerta de enlace que agregue todo, filtre por contexto y siga ejecutándose en segundo plano — todo controlable desde una hermosa UI nativa._
>
> _Moor expone un único endpoint (`http://127.0.0.1:<port>/mcp`) que sirve dinámicamente solo las herramientas que deseas, basado en tu Perfil activo. Cambia perfiles sin desconectar tu Agente, con la auditoría de llamadas a herramientas activada por defecto. Por eso lo construí._

<p align="center">
  <img src="./assets/Dashboard%20Page.png" alt="Dashboard" width="800"><br>
  <sub>Dashboard — Vista general del Perfil activo, estado de los servidores y estadísticas de auditoría.</sub>
</p>

<p align="center">
  <img src="./assets/Servers%20Page.png" alt="Servers" width="800"><br>
  <sub>Servers — Gestiona servidores MCP, importa configuraciones y monitoriza el estado.</sub>
</p>

<p align="center">
  <img src="./assets/Profiles%20Page.png" alt="Profiles" width="800"><br>
  <sub>Profiles — Crea perfiles, activa/desactiva servidores y habilita/deshabilita herramientas.</sub>
</p>

<p align="center">
  <img src="./assets/Audit%20Page.png" alt="Audit" width="800"><br>
  <sub>Audit — Inspecciona cada llamada a herramienta con contexto completo y filtros.</sub>
</p>

<a id="install"></a>

## Instalación

### Aplicación macOS

Descarga el `.dmg` desde [Releases](https://github.com/varandrew/moor/releases), arrástralo a Applications, listo. La aplicación integra un gateway HTTP en Rust dentro del proceso de escritorio — no se requiere Node.js preinstalado.

### Aplicación Windows

Descarga el instalador de Windows desde [Releases](https://github.com/varandrew/moor/releases) y ejecútalo. La aplicación integra un gateway HTTP en Rust dentro del proceso de escritorio — no se requiere Node.js preinstalado.

### Aplicación Linux

Descarga el `.deb`, `.rpm` o `.AppImage` desde [Releases](https://github.com/varandrew/moor/releases) (CI genera compilaciones para x86_64 y aarch64). La aplicación integra un gateway HTTP en Rust dentro del proceso de escritorio — no se requiere Node.js preinstalado. macOS sigue siendo la plataforma principal; las compilaciones de Linux se generan en CI, pero cuentan con una verificación menos exhaustiva.

### Compilar desde el código fuente

Requiere macOS (Apple Silicon / Intel), Windows x64 o Linux (x86_64 / aarch64), Node.js >= 22 (CI usa 24), pnpm 11.3.0 (fijado en `package.json`) y la cadena de herramientas estable de Rust utilizada en CI.

```bash
git clone https://github.com/varandrew/moor.git
cd moor
pnpm install
```

Consulta [Desarrollo](#development) para las instrucciones de compilación.

<a id="quickstart"></a>

## Inicio rápido

### Lanzar la aplicación

Abre **Moor** desde el menú de aplicaciones (`Moor.app` en macOS). El Dashboard muestra tu Perfil activo, el estado de los servidores y los registros de auditoría recientes de un vistazo.

### Escanear configuraciones existentes

Moor puede detectar automáticamente los servidores MCP que ya hayas configurado para Claude Code, Claude Desktop, Codex, OpenCode, Cursor, Kimi Code, dsh, Grok Build y Pi:

1. Ve a **Servers** → **Import**
2. Haz clic en **Scan** — Moor lee `~/.claude.json`, `~/.codex/config.toml`, `~/.config/opencode/opencode.json` / `.jsonc`, `~/.cursor/mcp.json`, `~/.kimi-code/mcp.json`, `~/.dsh/cordis.patch.yml`, `~/Library/Application Support/Claude/claude_desktop_config.json`, `~/.grok/config.toml` y `~/.pi/agent/mcp.json` (Pi requiere `pi install npm:pi-mcp-adapter`)
3. Selecciona los servidores que deseas importar

También puedes pegar una configuración MCP en JSON con **Import JSON**. Moor importa servidores stdio y HTTP/SSE, e informa sobre entradas no soportadas como configuraciones OpenAPI sin guardarlas.

Inicia los servidores importados desde **Servers** para que Moor descubra sus herramientas. Activar un servidor en un Perfil controla la exposición de herramientas; no inicia el servidor.

### Crear un Perfil

Los Perfiles te permiten agrupar servidores y controlar qué herramientas se exponen a los Agentes:

1. Ve a **Profiles** → **New Profile**
2. Ponle un nombre (p. ej., "Coding", "Research")
3. Activa/desactiva servidores
4. Expande un servidor para habilitar/deshabilitar herramientas individuales
5. Si lo deseas, elige **Curated listing** y selecciona las herramientas que quieres fijar; **Direct listing** es el modo predeterminado
6. Haz clic en **Activate** — el cambio es instantáneo

### Conectar tu Agente

Apunta cualquier cliente compatible con MCP al único endpoint de Moor:

```
http://127.0.0.1:9223/mcp
```

`9223` es el puerto del gateway por defecto. Si ya está en uso, Moor elige el siguiente puerto disponible y muestra el endpoint real en el Dashboard y las páginas de Client Config.

Por defecto, el endpoint `/mcp` es solo para loopback y no requiere `X-Moor-Token` (consulta el acceso LAN más abajo). Moor usa `X-Moor-Token` solo para APIs de gestión local entre el WebView y el gateway, por lo que no necesitas pegarlo en las configuraciones del agente.

Para conectarte desde WSL2 (modo NAT) u otro dispositivo de la LAN, activa **Settings → Advanced → Allow LAN MCP Access** y reinicia Moor. El gateway entonces escucha en todas las interfaces y `/mcp` acepta además hosts de redes privadas (`10.x.x.x`, `172.16.x.x`–`172.31.x.x`, `192.168.x.x`); `/api/*` sigue siendo solo loopback y requiere el token. Windows puede pedir permiso de firewall la primera vez.

Moor se encarga del resto — agregando `tools/list`, enrutando `tools/call`, y filtrando basado en tu Perfil activo.

### Ajustar los tiempos de espera de MCP

Si un servidor stdio se inicia mediante `npx`, `pnpm dlx` o un gestor de paquetes que descarga dependencias, su primer `initialize` / `tools/list` puede superar los 30 segundos predeterminados. En **Settings → Advanced**, aumenta **Server Start Timeout** para el arranque o **Request Timeout** para las solicitudes `tools/call` durante la ejecución. Ambos ajustes admiten entre 5 y 300 segundos.

Preinstala los servidores que uses con frecuencia y utiliza rutas absolutas para los comandos para reducir las esperas al arrancar. Para iniciarlos al abrir Moor, activa **Auto-start** en cada servidor y **Auto-start Servers on Launch** en Settings; solo se inician los servidores habilitados en el Perfil activo.

<a id="features"></a>

## Características

### Agregación de Gateway MCP

Un único endpoint HTTP (`/mcp`) hace de proxy para todos los servidores MCP backend. Los Agentes ven un catálogo de herramientas unificado — no es necesario configurar múltiples endpoints.

### Soporte multi-transporte

Conecta tanto servidores MCP **stdio** (subproceso) como **HTTP/SSE**. Moor gestiona las conexiones y el ciclo de vida de los servidores, con comprobaciones periódicas de salud opcionales.

### Gestión de Perfiles

Crea Perfiles ilimitados para diferentes flujos de trabajo. Cada Perfil almacena:

- Qué servidores están habilitados
- Qué herramientas están deshabilitadas por servidor
- Qué herramientas están fijadas, el modo de exposición y la política de riesgo
- Un estado activo global

Cambia Perfiles con **hot-swap** — los Agentes conectados permanecen conectados, y el siguiente `tools/list` refleja la nueva configuración inmediatamente.

### Conmutadores a nivel de herramienta

Más allá del encendido/apagado a nivel de servidor, profundiza en cualquier servidor para deshabilitar herramientas específicas. Las herramientas deshabilitadas quedan excluidas del catálogo del Agente y no se pueden llamar.

### Exposición selectiva y herramientas fijadas

**Direct listing** es el modo predeterminado para los Perfiles nuevos y existentes. Muestra todas las herramientas habilitadas, disponibles para llamadas y permitidas por la política del Perfil. **Curated listing** muestra solo las herramientas fijadas y dos metaherramientas del gateway:

- `moor_search_tools`: busca herramientas disponibles por palabras clave y nombre de servidor, con paginación y esquemas de entrada en los resultados
- `moor_call_tool`: llama a una herramienta descubierta por su exposed name y devuelve el resultado del servidor de destino

Usa los controles del Perfil para fijar herramientas individuales o las de un servidor en el catálogo predeterminado. Al cambiar a Curated listing se abre un selector para las herramientas que se fijarán inicialmente. Sin herramientas fijadas, el catálogo sigue incluyendo las dos metaherramientas. Las herramientas no fijadas siguen siendo accesibles mediante búsqueda y llamadas, incluso usando un exposed name guardado en caché; las deshabilitadas o bloqueadas por la política no lo son. Puedes volver a Direct listing en cualquier momento.

### Política de riesgo de herramientas

Moor clasifica las herramientas como `safe`, `write`, `destructive` o `unknown` a partir de sus nombres y descripciones. Puedes corregir la clasificación en la página de detalle del servidor.

Cada Perfil usa **Advisory** por defecto: muestra el riesgo sin bloquear llamadas. **Block destructive** excluye de los listados y resultados de búsqueda las herramientas clasificadas como `destructive`, y rechaza las llamadas con el código de error `-32002` y un motivo estructurado. La política se aplica en ambos modos de exposición y tiene prioridad sobre las herramientas fijadas. No bloquea herramientas `write` ni `unknown`; la clasificación es heurística y no garantiza un acceso de solo lectura.

### Importación de configuración

Importación con un solo clic desde:

- **Claude Code**: `~/.claude.json`
- **Codex**: `~/.codex/config.toml`
- **OpenCode**: `~/.config/opencode/opencode.json` / `.jsonc`
- **Cursor**: `~/.cursor/mcp.json`
- **Kimi Code**: `~/.kimi-code/mcp.json`
- **dsh**: `~/.dsh/cordis.patch.yml`
- **Claude Desktop**: `~/Library/Application Support/Claude/claude_desktop_config.json`
- **Grok Build**: `~/.grok/config.toml`
- **Pi**: `~/.pi/agent/mcp.json` (requiere `pi install npm:pi-mcp-adapter`)

También se admiten la entrada manual y la importación por lotes mediante JSON pegado para servidores stdio y HTTP/SSE.

### Configuración de cliente

Genera fragmentos de configuración listos para copiar para Claude Code, Claude Desktop, Codex, OpenCode, Cursor, Kimi Code, dsh, Grok Build y Pi. Los fragmentos contienen solo el endpoint `/mcp`; el `X-Moor-Token` de Moor está reservado para llamadas a la API de gestión interna.

### Registros de auditoría

Con la auditoría activada (por defecto), las llamadas a herramientas se registran con:

- Marca de tiempo, Perfil, Servidor, Nombre de herramienta
- Argumentos (con redacción de datos sensibles)
- Resultado o error
- Duración e información del Agente
- Origen de la llamada (`direct` o `meta`) y motivo del rechazo cuando se bloquea

Filtra por rango de tiempo, servidor o herramienta. El Dashboard muestra estadísticas agregadas e Insights incluye el número de llamadas bloqueadas por la política. Puedes desactivar la auditoría en **Settings → Advanced**. Los registros se conservan durante 30 días por defecto; establece `logRetentionDays` en `0` para conservarlos indefinidamente.

### Bandeja del sistema

Por defecto, cerrar la ventana oculta Moor en la barra de menús de macOS o en la bandeja del sistema de Windows mientras el gateway sigue funcionando. Desactiva **Minimize to Tray on Close** en Settings para salir al cerrar la ventana, o utiliza **Quit Moor** en el menú de la bandeja. Salir de la aplicación detiene el gateway.

### Estado en tiempo real

Los cambios de estado del servidor y los cambios de Perfil se envían a la UI mediante SSE. No es necesario refrescar.

Los clientes MCP que escuchan en `GET /mcp` reciben `notifications/tools/list_changed` cuando cambian el Perfil, las herramientas fijadas, la política de riesgo o el catálogo de herramientas. Los clientes que procesan esta notificación pueden obtener el catálogo actualizado sin reconectarse. Actualiza las herramientas desde la página de detalle de un servidor en ejecución para repetir el descubrimiento sin reiniciar el proceso.

<a id="architecture"></a>

## Arquitectura

<details>
<summary>Diagrama de arquitectura</summary>

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

### Flujo de comunicación

```
AI Agent ──HTTP──▶ POST /mcp ──▶ Moor Gateway ──stdio/HTTP──▶ MCP Servers
                              │
WebView ──IPC──▶ get_sidecar_info ─┐
WebView ──env──▶ VITE_MOOR_API_URL/TOKEN ─┤ (fallback en modo de desarrollo del navegador)
WebView ──fetch──▶ /api/* ─────────┘
WebView ◀──SSE──── /api/events
```

- **Detección de runtime**: WebView → Tauri IPC (`get_sidecar_info`) → Rust (puerto, token); en modo de desarrollo del navegador (`pnpm dev`) recurre a las variables de entorno `VITE_MOOR_API_URL` / `VITE_MOOR_API_TOKEN` (ver `src/lib/api/runtime.ts`; `GET /api/runtime` existe pero requiere token)
- **Operaciones de negocio**: WebView → HTTP `fetch()` → Servidor Axum en proceso (Rust)
- **Operaciones de sistema**: WebView → Tauri IPC → Rust (bandeja, ventana, auto-inicio)

<a id="development"></a>

## Desarrollo

### Requisitos previos

- macOS (Apple Silicon / Intel), Windows x64 o Linux (x86_64 / aarch64)
- [Node.js](https://nodejs.org) >= 22 (CI usa 24)
- [pnpm](https://pnpm.io) 11.3.0 (fijado en `package.json`)
- [Rust](https://rustup.rs) estable (la cadena de herramientas utilizada en CI)
- [Xcode Command Line Tools](https://developer.apple.com/xcode/resources/) en macOS

### Instalar dependencias

```bash
pnpm install
```

### Modo de desarrollo

Inicia la aplicación de escritorio completa (Tauri, con la pasarela Rust en proceso):

```bash
pnpm tauri dev
```

- Frontend: http://localhost:1420 (Vite HMR dentro del WebView)

### Compilación de producción

```bash
pnpm tauri build
```

Salidas:

- macOS: `src-tauri/target/release/bundle/macos/Moor.app`
- macOS DMG: `src-tauri/target/release/bundle/dmg/Moor_<version>_aarch64.dmg`
- Windows: `src-tauri/target/release/bundle/nsis/Moor_<version>_x64-setup.exe`
- Linux: `.deb` / `.rpm` / `.AppImage` en `src-tauri/target/release/bundle/` (x86_64 + aarch64, generados por CI)

### Calidad de código

```bash
vp check       # formato + lint + comprobación de tipos
vp lint        # solo lint
vp lint --fix  # auto-corrección
vp fmt         # formato
```

### Pruebas

```bash
# Pruebas de la pasarela Rust
cargo test --manifest-path src-tauri/Cargo.toml

# Pruebas de frontend + scripts
vp test
```

<a id="api"></a>

## API

Los endpoints de gestión `/api/*` requieren un Host de loopback y la cabecera `X-Moor-Token`. El endpoint `/mcp` sigue las reglas de acceso loopback/LAN descritas anteriormente y no requiere ese token.

### Gateway MCP

| Método | Ruta   | Descripción                                 |
| ------ | ------ | ------------------------------------------- |
| `ALL`  | `/mcp` | Endpoint de protocolo MCP (Streamable HTTP) |

### Gestión de servidores

| Método   | Ruta                                    | Descripción                                                          |
| -------- | --------------------------------------- | -------------------------------------------------------------------- |
| `GET`    | `/api/servers`                          | Listar todos los servidores                                          |
| `POST`   | `/api/servers`                          | Añadir servidor                                                      |
| `GET`    | `/api/servers/:id`                      | Detalle del servidor                                                 |
| `PUT`    | `/api/servers/:id`                      | Actualizar config del servidor                                       |
| `DELETE` | `/api/servers/:id`                      | Eliminar servidor                                                    |
| `POST`   | `/api/servers/:id/start`                | Iniciar servidor                                                     |
| `POST`   | `/api/servers/:id/stop`                 | Detener servidor                                                     |
| `GET`    | `/api/servers/:id/tools`                | Obtener herramientas descubiertas (filtro opcional `?profile_id=`)   |
| `POST`   | `/api/servers/:id/tools/refresh`        | Redescubrir herramientas de un servidor en ejecución sin reiniciarlo |
| `PATCH`  | `/api/servers/:id/tools/:toolName/risk` | Sobrescribir el `riskLevel` de una herramienta                       |
| `PUT`    | `/api/servers/order`                    | Reordenar servidores                                                 |

### Gestión de Perfiles

| Método   | Ruta                                  | Descripción                                                                                 |
| -------- | ------------------------------------- | ------------------------------------------------------------------------------------------- |
| `GET`    | `/api/profiles`                       | Listar todos los perfiles                                                                   |
| `POST`   | `/api/profiles`                       | Crear perfil                                                                                |
| `GET`    | `/api/profiles/:id`                   | Detalle del perfil (con servidores)                                                         |
| `PUT`    | `/api/profiles/:id`                   | Actualizar nombre, `exposureMode`, `riskPolicy` o `initialPins` al entrar en modo selectivo |
| `DELETE` | `/api/profiles/:id`                   | Eliminar perfil                                                                             |
| `PUT`    | `/api/profiles/:id/activate`          | Establecer como perfil activo                                                               |
| `POST`   | `/api/profiles/:id/clone`             | Clonar un perfil                                                                            |
| `GET`    | `/api/profiles/:id/servers/:serverId` | Obtener estado de activación del servidor, herramientas deshabilitadas y fijadas            |
| `PUT`    | `/api/profiles/:id/servers/:serverId` | Actualizar conmutador de servidor + herramientas deshabilitadas                             |
| `PUT`    | `/api/profiles/:id/servers-state`     | Actualizar en lote estados de activación, `disabledTools` y `pinnedTools`                   |
| `GET`    | `/api/profiles/:id/tools`             | Grupos de herramientas del Perfil con visibilidad, fijación y metadatos de riesgo           |

### Registros de auditoría

| Método | Ruta                 | Descripción                                       |
| ------ | -------------------- | ------------------------------------------------- |
| `GET`  | `/api/logs`          | Consultar logs (con filtros)                      |
| `GET`  | `/api/logs/stats`    | Estadísticas agregadas                            |
| `GET`  | `/api/logs/insights` | Insights de gobernanza de llamadas a herramientas |

### Gestión de configuración

| Método  | Ruta                  | Descripción                   |
| ------- | --------------------- | ----------------------------- |
| `GET`   | `/api/settings`       | Obtener configuración         |
| `PATCH` | `/api/settings`       | Actualizar config             |
| `POST`  | `/api/settings/reset` | Restaurar valores por defecto |

### Otros

| Método | Ruta                   | Descripción                              |
| ------ | ---------------------- | ---------------------------------------- |
| `GET`  | `/api/health`          | Verificación de salud                    |
| `GET`  | `/api/runtime`         | Info de runtime (puerto, URL)            |
| `GET`  | `/api/events`          | Stream de eventos SSE en tiempo real     |
| `POST` | `/api/import/scan`     | Escanear configs locales de clientes     |
| `POST` | `/api/import/parse`    | Previsualizar importación de JSON pegado |
| `POST` | `/api/import/execute`  | Ejecutar importación                     |
| `GET`  | `/api/import/snippets` | Generar snippets de config de clientes   |
| `POST` | `/api/import/convert`  | Convertir configs entre clientes         |

## Stack tecnológico

| Capa          | Tecnología                                           |
| ------------- | ---------------------------------------------------- |
| Frontend      | React 19, vite-plus, TypeScript 5.7, Tailwind CSS v4 |
| UI Primitives | Radix UI                                             |
| UI Components | shadcn/ui (New York style)                           |
| Desktop       | Tauri 2 (Rust)                                       |
| Gateway       | Rust, Axum, Tokio, rusqlite (en proceso)             |
| Database      | SQLite (rusqlite)                                    |
| MCP Protocol  | Rust nativo (JSON-RPC sobre Streamable HTTP / stdio) |
| Icons         | Lucide React                                         |
| Tooling       | vite-plus (vp CLI), Oxlint, Oxfmt, Vitest            |

## Agradecimientos

Gracias a la comunidad [linuxdo](https://linux.do/) por las discusiones, compartir y retroalimentación.

## ❤️ Patrocinio

[![ko-fi](https://ko-fi.com/img/githubbutton_sm.svg)](https://ko-fi.com/varandrew)

## 🌟 Historial de estrellas

[![Star History Chart](https://api.star-history.com/svg?repos=varandrew/moor&type=Date)](https://www.star-history.com/#varandrew/moor&Date)

## Licencia

[MIT](LICENSE)
