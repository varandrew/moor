use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ClientMeta {
    pub id: &'static str,
    pub name: &'static str,
    pub config_path_segments: &'static [&'static [&'static str]],
    pub format: &'static str, // "json" | "toml" | "yaml"
    pub top_level_key: &'static str,
    // Moor 对外发布的规范 entry 名；kimi-code/dsh 把 entry 固定写为
    // "moor-mcp"，重导入时替换手写 entry 而不是新建第二条网关连接。
    pub gateway_entry_name: &'static str,
    pub description: &'static str,
}

pub fn resolve_config_paths(client: &ClientMeta) -> Vec<PathBuf> {
    let home = dirs_home();
    client
        .config_path_segments
        .iter()
        .map(|segments| segments.iter().fold(home.clone(), |acc, s| acc.join(s)))
        .collect()
}

fn dirs_home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
}

pub const ALL_CLIENTS: &[ClientMeta] = &[
    ClientMeta {
        id: "claude-code",
        name: "Claude Code",
        config_path_segments: &[&[".claude.json"]],
        format: "json",
        top_level_key: "mcpServers",
        gateway_entry_name: "moor",
        description: "Add to ~/.claude.json → mcpServers",
    },
    ClientMeta {
        id: "codex",
        name: "Codex",
        config_path_segments: &[&[".codex", "config.toml"]],
        format: "toml",
        top_level_key: "mcp_servers",
        gateway_entry_name: "moor",
        description: "Add to ~/.codex/config.toml or project .codex/config.toml",
    },
    ClientMeta {
        id: "opencode",
        name: "OpenCode",
        config_path_segments: &[
            &[".config", "opencode", "opencode.json"],
            &[".config", "opencode", "opencode.jsonc"],
        ],
        format: "json",
        top_level_key: "mcp",
        gateway_entry_name: "moor",
        description: "Add to ~/.config/opencode/opencode.json or project opencode.json",
    },
    ClientMeta {
        id: "cursor",
        name: "Cursor",
        config_path_segments: &[&[".cursor", "mcp.json"]],
        format: "json",
        top_level_key: "mcpServers",
        gateway_entry_name: "moor",
        description: "Add to ~/.cursor/mcp.json or project .cursor/mcp.json",
    },
    ClientMeta {
        id: "kimi-code",
        name: "Kimi Code",
        config_path_segments: &[&[".kimi-code", "mcp.json"]],
        format: "json",
        top_level_key: "mcpServers",
        gateway_entry_name: "moor-mcp",
        description: "Add to ~/.kimi-code/mcp.json or project .kimi-code/mcp.json",
    },
    // MCP servers are Cordis plugin rows in the home-level patch layer; an
    // invalid patch file makes dsh fail to boot, so snippets are append-only.
    ClientMeta {
        id: "dsh",
        name: "DeepSeek Harness (dsh)",
        config_path_segments: &[&[".dsh", "cordis.patch.yml"]],
        format: "yaml",
        top_level_key: "insert",
        gateway_entry_name: "moor-mcp",
        description: "Append to ~/.dsh/cordis.patch.yml (export only; YAML import is unsupported)",
    },
    ClientMeta {
        id: "grok-build",
        name: "Grok Build",
        config_path_segments: &[&[".grok", "config.toml"]],
        format: "toml",
        top_level_key: "mcp_servers",
        gateway_entry_name: "moor",
        description: "Add to ~/.grok/config.toml or project .grok/config.toml (or: grok mcp add --transport http moor <url>)",
    },
    // Pi 原生 MCP 与项目配置共用 mcpServers 结构。
    ClientMeta {
        id: "pi",
        name: "Pi",
        config_path_segments: &[&[".pi", "agent", "mcp.json"]],
        format: "json",
        top_level_key: "mcpServers",
        gateway_entry_name: "moor",
        description: "Add to ~/.pi/agent/mcp.json or project .pi/mcp.json (native MCP)",
    },
    ClientMeta {
        id: "zcode",
        name: "ZCode",
        config_path_segments: &[&[".zcode", "cli", "config.json"], &[".agents", "mcp.json"]],
        format: "json",
        top_level_key: "mcp.servers",
        gateway_entry_name: "moor",
        description: "Add to ~/.zcode/cli/config.json → mcp.servers or project .zcode/config.json",
    },
    ClientMeta {
        id: "minimax-code",
        name: "MiniMax Code",
        config_path_segments: &[&[".minimax", "mcp.json"]],
        format: "json",
        top_level_key: "mcpServers",
        gateway_entry_name: "moor",
        description: "Add to ~/.minimax/mcp.json or paste the project .mcp.json configuration",
    },
    // claude_desktop_config.json accepts stdio servers only — the formatter
    // bridges HTTP through `npx mcp-remote`. Windows uses %APPDATA%\Claude,
    // which the home-relative segments model only on macOS.
    ClientMeta {
        id: "claude-desktop",
        name: "Claude Desktop",
        config_path_segments: &[&["Library", "Application Support", "Claude", "claude_desktop_config.json"]],
        format: "json",
        top_level_key: "mcpServers",
        gateway_entry_name: "moor",
        description: "Add to ~/Library/Application Support/Claude/claude_desktop_config.json (stdio via mcp-remote)",
    },
];

pub fn get_client_by_id(id: &str) -> Option<&'static ClientMeta> {
    ALL_CLIENTS.iter().find(|c| c.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    // 新增 client 必须附带 docs/{id}-mcp-docs.md（ClientConfig 四步 checklist 第 1 步）。
    #[test]
    fn every_client_has_a_docs_file() {
        for c in ALL_CLIENTS {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../docs")
                .join(format!("{}-mcp-docs.md", c.id));
            assert!(path.is_file(), "missing {}", path.display());
        }
    }
}
