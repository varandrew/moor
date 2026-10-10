use super::clients::{self, ClientMeta};
use super::import_parser::{self, ImportDiagnostic, ParsedImport};
use std::fs;

use super::import_parser::ScannedServer;

fn read_file_if_exists(path: &std::path::Path) -> Option<String> {
    if !path.exists() {
        return None;
    }
    fs::read_to_string(path).ok()
}

fn parse_config_file(config_path: &std::path::Path, source: &str, format: &str) -> ParsedImport {
    let content = match read_file_if_exists(config_path) {
        Some(c) => c,
        None => return ParsedImport::default(),
    };
    let parsed = match format {
        "toml" => import_parser::parse_codex_toml_config(&content, source),
        // dsh patches carry `!!js` dynamic expressions that cannot be statically
        // evaluated; surface a diagnostic instead of returning silently empty.
        "yaml" => {
            return ParsedImport {
                diagnostics: vec![ImportDiagnostic {
                    source: source.to_string(),
                    message: "YAML config uses dynamic `!!js` expressions and cannot be statically scanned.".to_string(),
                    code: None,
                    line: None,
                    column: None,
                    offset: None,
                    length: None,
                }],
                ..ParsedImport::default()
            };
        }
        _ => import_parser::parse_json_mcp_config(&content, source),
    };
    ignore_missing_mcp_sections(parsed, source)
}

fn ignore_missing_mcp_sections(result: ParsedImport, source: &str) -> ParsedImport {
    let missing_errors: std::collections::HashSet<String> = [
        format!("{source}: no mcpServers or mcp key found"),
        format!("{source}: no mcp_servers key found"),
    ]
    .into_iter()
    .collect();

    let only_missing = result.servers.is_empty()
        && result.unsupported.is_empty()
        && !result.errors.is_empty()
        && result.errors.iter().all(|e| missing_errors.contains(e));

    if only_missing {
        ParsedImport::default()
    } else {
        result
    }
}

fn scan_client(client: &ClientMeta) -> ParsedImport {
    let paths = clients::resolve_config_paths(client);
    if client.id == "zcode" {
        return scan_zcode_paths(&paths[0], &paths[1]);
    }
    let results: Vec<ParsedImport> = paths
        .iter()
        .map(|p| parse_config_file(p, client.id, client.format))
        .collect();

    let mut merged = merge_results(&results);

    if paths.len() > 1 {
        let mut seen = std::collections::HashSet::new();
        merged.servers.retain(|s| seen.insert(s.name.clone()));
    }

    merged
}

fn scan_zcode_paths(native: &std::path::Path, fallback: &std::path::Path) -> ParsedImport {
    if let Some(content) = read_file_if_exists(native) {
        // 原生配置存在条目时整份兼容文件被遮蔽，禁用条目也算存在。
        let has_servers = serde_json::from_str::<serde_json::Value>(&content)
            .map(|root| {
                root.pointer("/mcp/servers")
                    .is_some_and(|v| v.as_object().map_or(true, |m| !m.is_empty()))
            })
            .unwrap_or(true);
        if has_servers {
            return parse_config_file(native, "zcode", "json");
        }
    }
    parse_config_file(fallback, "zcode", "json")
}

pub fn scan_all_configs() -> ParsedImport {
    let results: Vec<ParsedImport> = clients::ALL_CLIENTS.iter().map(scan_client).collect();
    merge_results(&results)
}

pub fn scan_client_config(client_id: &str) -> ParsedImport {
    match clients::get_client_by_id(client_id) {
        Some(client) => scan_client(client),
        None => ParsedImport::default(),
    }
}

fn merge_results(results: &[ParsedImport]) -> ParsedImport {
    let mut merged = ParsedImport::default();
    for r in results {
        merged.servers.extend(r.servers.clone());
        merged.unsupported.extend(r.unsupported.clone());
        merged.errors.extend(r.errors.clone());
        merged.diagnostics.extend(r.diagnostics.clone());
    }
    merged
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub scanned: usize,
    pub new_servers: usize,
    pub servers: Vec<ScannedServer>,
    pub duplicates: Vec<ScannedServer>,
    pub unsupported: Vec<super::import_parser::UnsupportedServer>,
    pub errors: Vec<String>,
    pub diagnostics: Vec<super::import_parser::ImportDiagnostic>,
}

pub fn build_import_preview(
    parsed: &ParsedImport,
    existing_names: &std::collections::HashSet<String>,
) -> ImportPreview {
    let mut seen = existing_names.clone();
    let mut servers = vec![];
    let mut duplicates = vec![];

    for server in &parsed.servers {
        if seen.contains(&server.name) {
            duplicates.push(server.clone());
        } else {
            seen.insert(server.name.clone());
            let mut imported = server.clone();
            // 入库仍使用 HTTP；转换保留 SSE，网关传输层负责协议识别。
            if imported.connection_type == "sse" {
                imported.connection_type = "http".to_string();
            }
            servers.push(imported);
        }
    }

    ImportPreview {
        scanned: parsed.servers.len() + parsed.unsupported.len(),
        new_servers: servers.len(),
        servers,
        duplicates,
        unsupported: parsed.unsupported.clone(),
        errors: parsed.errors.clone(),
        diagnostics: parsed.diagnostics.clone(),
    }
}

pub fn partition_import_candidates(
    servers: &[ScannedServer],
    existing_names: &std::collections::HashSet<String>,
) -> (Vec<ScannedServer>, Vec<String>) {
    let mut seen = existing_names.clone();
    let mut candidates = vec![];
    let mut skipped = vec![];

    for server in servers {
        if seen.contains(&server.name) {
            skipped.push(server.name.clone());
        } else {
            seen.insert(server.name.clone());
            candidates.push(server.clone());
        }
    }

    (candidates, skipped)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scanned_server(name: &str) -> ScannedServer {
        ScannedServer {
            name: name.to_string(),
            connection_type: "stdio".to_string(),
            source: "test".to_string(),
            command: Some("node".to_string()),
            args: Some(vec!["server.js".to_string()]),
            url: None,
            env: None,
            headers: None,
            working_dir: None,
        }
    }

    #[test]
    fn sse_preview_keeps_the_existing_http_import_contract() {
        let mut server = scanned_server("legacy");
        server.connection_type = "sse".to_string();
        let parsed = ParsedImport {
            servers: vec![server],
            ..Default::default()
        };
        let preview = build_import_preview(&parsed, &Default::default());
        assert_eq!(preview.servers[0].connection_type, "http");
        assert_eq!(parsed.servers[0].connection_type, "sse");
    }

    #[test]
    fn zcode_native_entries_shadow_fallback_even_when_disabled() {
        let dir = std::env::temp_dir().join(format!(
            "moor-zcode-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let native = dir.join("config.json");
        let fallback = dir.join("mcp.json");
        std::fs::write(
            &fallback,
            r#"{"mcpServers":{"fallback":{"url":"http://localhost/mcp"}}}"#,
        )
        .unwrap();
        std::fs::write(
            &native,
            r#"{"mcp":{"servers":{"off":{"url":"http://localhost/mcp","enable":false}}}}"#,
        )
        .unwrap();
        assert!(scan_zcode_paths(&native, &fallback).servers.is_empty());
        std::fs::write(&native, r#"{"mcp":{"servers":{}}}"#).unwrap();
        assert_eq!(
            scan_zcode_paths(&native, &fallback).servers[0].name,
            "fallback"
        );
        std::fs::write(&native, "{broken").unwrap();
        assert!(!scan_zcode_paths(&native, &fallback).errors.is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn partitions_duplicate_import_names() {
        let existing_names = std::collections::HashSet::from(["existing".to_string()]);
        let (candidates, skipped) = partition_import_candidates(
            &[scanned_server("existing"), scanned_server("new")],
            &existing_names,
        );
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].name, "new");
        assert_eq!(skipped, vec!["existing"]);
    }
}
