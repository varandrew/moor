use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScannedServer {
    pub name: String,
    pub connection_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<std::collections::HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<std::collections::HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnsupportedServer {
    pub name: String,
    pub source: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportDiagnostic {
    pub source: String,
    pub message: String,
    pub code: Option<String>,
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub offset: Option<usize>,
    pub length: Option<usize>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedImport {
    pub servers: Vec<ScannedServer>,
    pub unsupported: Vec<UnsupportedServer>,
    pub errors: Vec<String>,
    pub diagnostics: Vec<ImportDiagnostic>,
}

const HTTP_TYPES: &[&str] = &[
    "http",
    "sse",
    "streamable-http",
    "streamable_http",
    "remote",
];

pub fn parse_json_mcp_config(content: &str, source: &str) -> ParsedImport {
    let stripped = strip_jsonc(content);
    let config: Value = match serde_json::from_str(&stripped) {
        Ok(v) => v,
        Err(e) => {
            let line = e.line();
            let column = e.column();
            return ParsedImport {
                servers: vec![],
                unsupported: vec![],
                errors: vec![if line > 0 && column > 0 {
                    format!("{source}: JSON parse error at line {line}, column {column}")
                } else {
                    format!("{source}: JSON parse error")
                }],
                diagnostics: vec![ImportDiagnostic {
                    source: source.to_string(),
                    message: e.to_string(),
                    code: None,
                    line: Some(line),
                    column: Some(column),
                    offset: None,
                    length: None,
                }],
            };
        }
    };

    let config_obj = match config.as_object() {
        Some(obj) => obj,
        None => {
            return ParsedImport {
                errors: vec![format!("{source}: config root must be an object")],
                ..Default::default()
            }
        }
    };

    let mut results = Vec::new();
    if let Some(mcp_servers) = config_obj.get("mcpServers") {
        results.push(parse_server_map(mcp_servers, source));
    }
    if let Some(mcp) = config_obj.get("mcp") {
        if let Some(servers) = mcp.get("servers").filter(|servers| {
            source == "zcode"
                || ((source == "paste" || source == "json-import")
                    && servers
                        .as_object()
                        .is_some_and(|m| m.values().all(Value::is_object)))
        }) {
            results.push(parse_server_map(servers, "zcode"));
        } else {
            results.push(parse_server_map(mcp, source));
        }
    }

    if results.is_empty() {
        return ParsedImport {
            errors: vec![format!("{source}: no mcpServers or mcp key found")],
            ..Default::default()
        };
    }

    let mut parsed = merge_parsed(&results);
    if source == "json-import" {
        for server in &mut parsed.servers {
            if server.connection_type == "sse" {
                server.connection_type = "http".to_string();
            }
        }
    }
    parsed
}

pub fn parse_codex_toml_config(content: &str, source: &str) -> ParsedImport {
    let config: toml::Value = match toml::from_str(content) {
        Ok(v) => v,
        Err(_) => {
            return ParsedImport {
                errors: vec![format!("{source}: TOML parse error")],
                ..Default::default()
            }
        }
    };

    let config_table = match config.as_table() {
        Some(t) => t,
        None => {
            return ParsedImport {
                errors: vec![format!("{source}: TOML root must be an object")],
                ..Default::default()
            }
        }
    };

    match config_table.get("mcp_servers") {
        Some(mcp_servers) => {
            let json_val = toml_to_json_value(mcp_servers);
            let mut parsed = parse_server_map(&json_val, source);
            if source == "grok-build" {
                if let Some(disabled) = config_table
                    .get("disabled_mcp_servers")
                    .and_then(toml::Value::as_array)
                {
                    parsed
                        .servers
                        .retain(|s| !disabled.iter().any(|name| name.as_str() == Some(&s.name)));
                }
            }
            parsed
        }
        None => ParsedImport {
            errors: vec![format!("{source}: no mcp_servers key found")],
            ..Default::default()
        },
    }
}

fn toml_to_json_value(val: &toml::Value) -> serde_json::Value {
    match val {
        toml::Value::String(s) => serde_json::Value::String(s.clone()),
        toml::Value::Integer(i) => serde_json::json!(*i),
        toml::Value::Float(f) => serde_json::json!(*f),
        toml::Value::Boolean(b) => serde_json::json!(*b),
        toml::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(toml_to_json_value).collect())
        }
        toml::Value::Table(tbl) => {
            let mut map = serde_json::Map::new();
            for (k, v) in tbl {
                map.insert(k.clone(), toml_to_json_value(v));
            }
            serde_json::Value::Object(map)
        }
        toml::Value::Datetime(dt) => serde_json::Value::String(dt.to_string()),
    }
}

fn parse_server_map(value: &Value, source: &str) -> ParsedImport {
    let obj = match value.as_object() {
        Some(o) => o,
        None => {
            return ParsedImport {
                errors: vec![format!("{source}: no valid server map found")],
                ..Default::default()
            }
        }
    };

    let mut servers = vec![];
    let mut unsupported = vec![];
    let mut diagnostics = vec![];

    for (name, raw_config) in obj {
        let raw_obj = match raw_config.as_object() {
            Some(o) => o,
            None => {
                unsupported.push(UnsupportedServer {
                    name: name.clone(),
                    source: source.to_string(),
                    reason: "server config must be an object".to_string(),
                });
                continue;
            }
        };

        if raw_obj.get("enabled").and_then(|v| v.as_bool()) == Some(false)
            || (source == "zcode" && raw_obj.get("enable").and_then(|v| v.as_bool()) == Some(false))
        {
            continue;
        }

        match normalize_server(name, raw_obj, source) {
            Some(Normalized::Server(s)) => {
                let unmapped: Vec<_> = raw_obj
                    .keys()
                    .filter(|key| {
                        ![
                            "type",
                            "transport",
                            "command",
                            "args",
                            "url",
                            "env",
                            "environment",
                            "headers",
                            "http_headers",
                            "env_http_headers",
                            "bearer_token_env_var",
                            "bearerTokenEnvVar",
                            "cwd",
                            "workingDir",
                            "working_dir",
                            "enabled",
                            "enable",
                        ]
                        .contains(&key.as_str())
                    })
                    .cloned()
                    .collect();
                if !unmapped.is_empty() {
                    diagnostics.push(ImportDiagnostic {
                        source: source.to_string(),
                        message: format!(
                            "Server {name}: client-specific fields are not converted: {}",
                            unmapped.join(", ")
                        ),
                        code: Some("UNMAPPED_FIELDS".to_string()),
                        line: None,
                        column: None,
                        offset: None,
                        length: None,
                    });
                }
                servers.push(s);
            }
            Some(Normalized::Unsupported(u)) => unsupported.push(u),
            None => {}
        }
    }

    ParsedImport {
        servers,
        unsupported,
        errors: vec![],
        diagnostics,
    }
}

enum Normalized {
    Server(ScannedServer),
    Unsupported(UnsupportedServer),
}

fn normalize_server(
    name: &str,
    raw: &serde_json::Map<String, Value>,
    source: &str,
) -> Option<Normalized> {
    let type_val = raw
        .get("type")
        .or_else(|| {
            if source == "kimi-code" {
                raw.get("transport")
            } else {
                None
            }
        })
        .and_then(|v| v.as_str())
        .map(|s| s.to_lowercase());

    if type_val.as_deref() == Some("openapi") || raw.contains_key("openapi") {
        return Some(Normalized::Unsupported(UnsupportedServer {
            name: name.to_string(),
            source: source.to_string(),
            reason: "OpenAPI-to-MCP is not supported".to_string(),
        }));
    }

    let command_array = raw.get("command").and_then(|v| v.as_array());
    let command = raw
        .get("command")
        .and_then(|v| v.as_str())
        .map(String::from)
        .or_else(|| {
            command_array.and_then(|arr| arr.first().and_then(|v| v.as_str()).map(String::from))
        });
    let args: Option<Vec<String>> = command_array
        .map(|arr| {
            arr.iter()
                .skip(1)
                .filter_map(|v| v.as_str().map(String::from))
                .collect::<Vec<String>>()
        })
        .filter(|a: &Vec<String>| !a.is_empty())
        .or_else(|| {
            raw.get("args").and_then(|v| v.as_array()).map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect::<Vec<String>>()
            })
        });
    let url = raw.get("url").and_then(|v| v.as_str()).map(String::from);
    let env = as_string_record(raw.get("env").or(raw.get("environment")));
    let working_dir = raw
        .get("cwd")
        .or(raw.get("workingDir"))
        .or(raw.get("working_dir"))
        .and_then(|v| v.as_str())
        .map(String::from);
    let bearer_token_env = raw
        .get("bearer_token_env_var")
        .or_else(|| raw.get("bearerTokenEnvVar"))
        .and_then(|v| v.as_str());
    let headers = merge_records(
        as_header_record(raw.get("headers")),
        as_header_record(raw.get("http_headers")),
        env_headers(raw.get("env_http_headers")),
        bearer_token_env.map(|var| {
            let mut map = std::collections::HashMap::new();
            map.insert("Authorization".to_string(), format!("Bearer {{env:{var}}}"));
            map
        }),
    );

    let is_local = type_val.as_deref() == Some("local") || type_val.as_deref() == Some("stdio");
    if is_local || command.is_some() {
        let command = match command {
            Some(c) => c,
            None => {
                return Some(Normalized::Unsupported(UnsupportedServer {
                    name: name.to_string(),
                    source: source.to_string(),
                    reason: "stdio server is missing a command".to_string(),
                }))
            }
        };
        return Some(Normalized::Server(ScannedServer {
            name: name.to_string(),
            connection_type: "stdio".to_string(),
            command: Some(command),
            args,
            url: None,
            env,
            headers: None,
            working_dir,
            source: source.to_string(),
        }));
    }

    if let Some(url) = url {
        if type_val
            .as_deref()
            .map(|t| HTTP_TYPES.contains(&t))
            .unwrap_or(true)
        {
            return Some(Normalized::Server(ScannedServer {
                name: name.to_string(),
                connection_type: if type_val.as_deref() == Some("sse") {
                    "sse"
                } else {
                    "http"
                }
                .to_string(),
                command: None,
                args: None,
                url: Some(url),
                env: None,
                headers,
                working_dir: None,
                source: source.to_string(),
            }));
        }
        return Some(Normalized::Unsupported(UnsupportedServer {
            name: name.to_string(),
            source: source.to_string(),
            reason: format!(
                "unsupported server type \"{}\"",
                type_val.unwrap_or_default()
            ),
        }));
    }

    if let Some(ref t) = type_val {
        if !HTTP_TYPES.contains(&t.as_str()) {
            return Some(Normalized::Unsupported(UnsupportedServer {
                name: name.to_string(),
                source: source.to_string(),
                reason: format!("unsupported server type \"{t}\""),
            }));
        }
    }

    Some(Normalized::Unsupported(UnsupportedServer {
        name: name.to_string(),
        source: source.to_string(),
        reason: "config is missing command or url".to_string(),
    }))
}

fn as_string_record(value: Option<&Value>) -> Option<std::collections::HashMap<String, String>> {
    let obj = value?.as_object()?;
    let mut map = std::collections::HashMap::new();
    for (k, v) in obj {
        match v {
            Value::String(s) => {
                map.insert(k.clone(), s.clone());
            }
            Value::Number(n) => {
                map.insert(k.clone(), n.to_string());
            }
            Value::Bool(b) => {
                map.insert(k.clone(), b.to_string());
            }
            _ => continue,
        }
    }
    if map.is_empty() {
        None
    } else {
        Some(map)
    }
}

fn as_header_record(value: Option<&Value>) -> Option<std::collections::HashMap<String, String>> {
    let obj = value?.as_object()?;
    let mut map = std::collections::HashMap::new();
    for (k, v) in obj {
        match v {
            Value::String(s) => {
                map.insert(k.clone(), s.clone());
            }
            Value::Object(inner) => {
                if let Some(env_val) = inner.get("env").and_then(|v| v.as_str()) {
                    map.insert(k.clone(), format!("{{env:{env_val}}}"));
                }
            }
            _ => continue,
        }
    }
    if map.is_empty() {
        None
    } else {
        Some(map)
    }
}

fn env_headers(value: Option<&Value>) -> Option<std::collections::HashMap<String, String>> {
    let obj = value?.as_object()?;
    let mut map = std::collections::HashMap::new();
    for (k, v) in obj {
        if let Value::String(s) = v {
            map.insert(k.clone(), format!("{{env:{s}}}"));
        }
    }
    if map.is_empty() {
        None
    } else {
        Some(map)
    }
}

fn merge_records(
    a: Option<std::collections::HashMap<String, String>>,
    b: Option<std::collections::HashMap<String, String>>,
    c: Option<std::collections::HashMap<String, String>>,
    d: Option<std::collections::HashMap<String, String>>,
) -> Option<std::collections::HashMap<String, String>> {
    let mut merged = std::collections::HashMap::new();
    for map in [a, b, c, d].into_iter().flatten() {
        merged.extend(map);
    }
    if merged.is_empty() {
        None
    } else {
        Some(merged)
    }
}

fn merge_parsed(results: &[ParsedImport]) -> ParsedImport {
    let mut servers = vec![];
    let mut unsupported = vec![];
    let mut errors = vec![];
    let mut diagnostics = vec![];
    for r in results {
        servers.extend(r.servers.clone());
        unsupported.extend(r.unsupported.clone());
        errors.extend(r.errors.clone());
        diagnostics.extend(r.diagnostics.clone());
    }
    ParsedImport {
        servers,
        unsupported,
        errors,
        diagnostics,
    }
}

/// Strip JSONC comments (// and /* */) and trailing commas from JSON content.
fn strip_jsonc(content: &str) -> String {
    let mut result = String::with_capacity(content.len());
    let chars: Vec<char> = content.chars().collect();
    let len = chars.len();
    let mut i = 0;
    let mut in_string = false;

    while i < len {
        let c = chars[i];

        if in_string {
            result.push(c);
            if c == '\\' && i + 1 < len {
                i += 1;
                result.push(chars[i]);
            } else if c == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }

        match c {
            '"' => {
                in_string = true;
                result.push(c);
            }
            '/' if i + 1 < len && chars[i + 1] == '/' => {
                while i < len && chars[i] != '\n' {
                    i += 1;
                }
                continue;
            }
            '/' if i + 1 < len && chars[i + 1] == '*' => {
                i += 2;
                while i + 1 < len && !(chars[i] == '*' && chars[i + 1] == '/') {
                    i += 1;
                }
                i += 2;
                continue;
            }
            ',' if i + 1 < len => {
                let mut j = i + 1;
                while j < len && chars[j].is_whitespace() {
                    j += 1;
                }
                if j < len && (chars[j] == ']' || chars[j] == '}') {
                    i += 1;
                    continue;
                }
                result.push(c);
            }
            _ => {
                result.push(c);
            }
        }
        i += 1;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find<'a>(parsed: &'a ParsedImport, name: &str) -> &'a ScannedServer {
        parsed
            .servers
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("server {name} not found"))
    }

    #[test]
    fn automatic_paste_recognizes_zcode_without_masking_opencode_server_names() {
        let zcode = parse_json_mcp_config(
            r#"{"mcp":{"servers":{"off":{"enable":false,"url":"http://localhost/mcp"},"on":{"url":"http://localhost/mcp"}}}}"#,
            "json-import",
        );
        assert_eq!(zcode.servers.len(), 1);
        assert_eq!(zcode.servers[0].name, "on");
        let opencode = parse_json_mcp_config(
            r#"{"mcp":{"servers":{"type":"remote","url":"http://localhost/mcp"}}}"#,
            "json-import",
        );
        assert_eq!(opencode.servers[0].name, "servers");
    }

    #[test]
    fn zcode_disabled_fields_never_import_disabled_servers() {
        let parsed = parse_json_mcp_config(
            r#"{"mcp":{"servers":{
            "desktop-off":{"url":"http://localhost/mcp","enable":false,"enabled":true},
            "cli-off":{"url":"http://localhost/mcp","enabled":false},
            "active":{"url":"http://localhost/mcp","type":"http"}
        }}}"#,
            "zcode",
        );
        assert!(parsed.errors.is_empty());
        assert_eq!(parsed.servers.len(), 1);
        assert_eq!(parsed.servers[0].name, "active");
    }

    #[test]
    fn minimax_disabled_entries_and_sse_keep_their_intent() {
        let parsed = parse_json_mcp_config(
            r#"{"mcpServers":{
            "off":{"url":"http://localhost/mcp","enabled":false},
            "legacy":{"url":"http://localhost/sse","type":"sse","timeout":30000}
        }}"#,
            "minimax-code",
        );
        assert_eq!(parsed.servers.len(), 1);
        assert_eq!(parsed.servers[0].connection_type, "sse");
        assert!(parsed.diagnostics[0].message.contains("timeout"));
    }

    #[test]
    fn grok_root_disabled_list_is_respected() {
        let parsed = parse_codex_toml_config("disabled_mcp_servers = [\"off\"]\n[mcp_servers.off]\nurl = \"http://localhost/mcp\"\n[mcp_servers.on]\nurl = \"http://localhost/mcp\"", "grok-build");
        assert_eq!(parsed.servers.len(), 1);
        assert_eq!(parsed.servers[0].name, "on");
    }

    #[test]
    fn parses_supported_json_and_reports_openapi_as_unsupported() {
        let parsed = parse_json_mcp_config(
            r#"{
              "mcpServers": {
                "stdio-server-example": { "command": "npx", "args": ["-y", "mcp-server-example"] },
                "sse-server-example": { "type": "sse", "url": "http://localhost:3000" },
                "http-server-example": {
                  "type": "streamable-http",
                  "url": "http://localhost:3001",
                  "headers": { "Content-Type": "application/json", "Authorization": "Bearer your-token" }
                },
                "openapi-server-example": {
                  "type": "openapi",
                  "openapi": { "url": "https://petstore.swagger.io/v2/swagger.json" }
                }
              }
            }"#,
            "json-import",
        );

        assert!(parsed.errors.is_empty());
        assert_eq!(parsed.servers.len(), 3);

        let stdio = find(&parsed, "stdio-server-example");
        assert_eq!(stdio.connection_type, "stdio");
        assert_eq!(stdio.command.as_deref(), Some("npx"));
        assert_eq!(
            stdio.args,
            Some(vec!["-y".to_string(), "mcp-server-example".to_string()])
        );

        let sse = find(&parsed, "sse-server-example");
        assert_eq!(sse.connection_type, "http");
        assert_eq!(sse.url.as_deref(), Some("http://localhost:3000"));

        let http = find(&parsed, "http-server-example");
        assert_eq!(http.connection_type, "http");
        let headers = http.headers.as_ref().expect("http server has headers");
        assert_eq!(
            headers.get("Authorization").map(String::as_str),
            Some("Bearer your-token")
        );
        assert_eq!(
            headers.get("Content-Type").map(String::as_str),
            Some("application/json")
        );

        assert_eq!(parsed.unsupported.len(), 1);
        assert_eq!(parsed.unsupported[0].name, "openapi-server-example");
        assert_eq!(
            parsed.unsupported[0].reason,
            "OpenAPI-to-MCP is not supported"
        );
    }

    #[test]
    fn reports_json_parse_error_with_diagnostic() {
        // Node used jsonc-parser ("CommaExpected" + offset); Rust uses serde_json, so we assert
        // the behaviour (an error + a diagnostic), not the exact code/offset.
        let parsed = parse_json_mcp_config(
            "{\n  \"mcpServers\": {\n    \"broken\": {\n      \"command\": \"npx\"\n      \"args\": []\n    }\n  }\n}",
            "json-import",
        );
        assert!(parsed.servers.is_empty());
        assert!(parsed.unsupported.is_empty());
        assert_eq!(parsed.errors.len(), 1);
        assert!(parsed.errors[0].starts_with("json-import: JSON parse error"));
        assert_eq!(parsed.diagnostics.len(), 1);
        assert!(parsed.diagnostics[0].line.is_some());
    }

    #[test]
    fn parses_codex_toml_with_merged_and_env_headers() {
        let parsed = parse_codex_toml_config(
            r#"
[mcp_servers.code-review-graph]
command = "uvx"
args = ["code-review-graph", "serve"]
type = "stdio"

[mcp_servers.figma]
url = "https://mcp.figma.com/mcp"
http_headers = { "X-Figma-Region" = "us-east-1" }
env_http_headers = { "X-API-Key" = "FIGMA_TOKEN" }
bearer_token_env_var = "FIGMA_OAUTH_TOKEN"
"#,
            "codex",
        );

        assert!(parsed.errors.is_empty());
        let crg = find(&parsed, "code-review-graph");
        assert_eq!(crg.connection_type, "stdio");
        assert_eq!(crg.command.as_deref(), Some("uvx"));
        assert_eq!(
            crg.args,
            Some(vec!["code-review-graph".to_string(), "serve".to_string()])
        );

        let figma = find(&parsed, "figma");
        assert_eq!(figma.connection_type, "http");
        let headers = figma.headers.as_ref().expect("figma has headers");
        assert_eq!(
            headers.get("X-Figma-Region").map(String::as_str),
            Some("us-east-1")
        );
        assert_eq!(
            headers.get("X-API-Key").map(String::as_str),
            Some("{env:FIGMA_TOKEN}")
        );
        assert_eq!(
            headers.get("Authorization").map(String::as_str),
            Some("Bearer {env:FIGMA_OAUTH_TOKEN}")
        );
    }

    #[test]
    fn skips_disabled_json_and_toml_entries() {
        let json = parse_json_mcp_config(
            r#"{ "mcp": {
              "enabledRemote": { "type": "remote", "url": "https://enabled.example.com/mcp" },
              "disabledRemote": { "type": "remote", "url": "https://disabled.example.com/mcp", "enabled": false }
            }}"#,
            "opencode",
        );
        assert!(json.errors.is_empty());
        assert_eq!(
            json.servers
                .iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>(),
            vec!["enabledRemote"]
        );
        assert!(json.unsupported.is_empty());

        let toml = parse_codex_toml_config(
            r#"
[mcp_servers.enabled_stdio]
command = "uvx"
args = ["enabled-server"]

[mcp_servers.disabled_stdio]
command = "uvx"
args = ["disabled-server"]
enabled = false
"#,
            "codex",
        );
        assert!(toml.errors.is_empty());
        assert_eq!(
            toml.servers
                .iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>(),
            vec!["enabled_stdio"]
        );
        assert!(toml.unsupported.is_empty());
    }

    #[test]
    fn parses_opencode_jsonc_local_and_remote() {
        let parsed = parse_json_mcp_config(
            r#"
{
  "mcp": {
    "local-docs": {
      "type": "local",
      "command": ["bun", "x", "docs-mcp"],
      "environment": { "DOCS_TOKEN": "local" },
    },
    "remote-docs": {
      "type": "remote",
      "url": "https://docs.example.com/mcp",
      "headers": { "Authorization": "Bearer {env:DOCS_TOKEN}" }
    }
  }
}
"#,
            "opencode",
        );

        assert!(parsed.errors.is_empty());
        assert_eq!(parsed.servers.len(), 2);

        let local = find(&parsed, "local-docs");
        assert_eq!(local.connection_type, "stdio");
        assert_eq!(local.command.as_deref(), Some("bun"));
        assert_eq!(
            local.args,
            Some(vec!["x".to_string(), "docs-mcp".to_string()])
        );
        assert_eq!(
            local
                .env
                .as_ref()
                .and_then(|e| e.get("DOCS_TOKEN"))
                .map(String::as_str),
            Some("local")
        );

        let remote = find(&parsed, "remote-docs");
        assert_eq!(remote.connection_type, "http");
        assert_eq!(remote.url.as_deref(), Some("https://docs.example.com/mcp"));
        assert_eq!(
            remote
                .headers
                .as_ref()
                .and_then(|h| h.get("Authorization"))
                .map(String::as_str),
            Some("Bearer {env:DOCS_TOKEN}")
        );
    }

    #[test]
    fn parses_cursor_stdio_and_http() {
        let parsed = parse_json_mcp_config(
            r#"{
              "mcpServers": {
                "local-tool": { "type": "stdio", "command": "npx", "args": ["-y", "my-mcp-server"], "env": { "API_KEY": "test" } },
                "remote-tool": { "url": "https://mcp.example.com/mcp" }
              }
            }"#,
            "cursor",
        );

        assert!(parsed.errors.is_empty());
        assert_eq!(parsed.servers.len(), 2);

        let local = find(&parsed, "local-tool");
        assert_eq!(local.connection_type, "stdio");
        assert_eq!(local.command.as_deref(), Some("npx"));
        assert_eq!(
            local
                .env
                .as_ref()
                .and_then(|e| e.get("API_KEY"))
                .map(String::as_str),
            Some("test")
        );

        let remote = find(&parsed, "remote-tool");
        assert_eq!(remote.connection_type, "http");
        assert_eq!(remote.url.as_deref(), Some("https://mcp.example.com/mcp"));
    }

    #[test]
    fn parses_kimi_code_mcp_servers() {
        // Kimi Code: entries with `command` are stdio; bare `url` means HTTP (no transport field).
        let parsed = parse_json_mcp_config(
            r#"{
              "mcpServers": {
                "filesystem": {
                  "command": "npx",
                  "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"],
                  "env": { "FS_TOKEN": "abc" },
                  "cwd": "/tmp"
                },
                "linear": { "url": "https://mcp.linear.app/mcp" },
                "legacy-events": { "transport": "sse", "url": "https://mcp.example.com/sse" }
              }
            }"#,
            "kimi-code",
        );

        assert!(parsed.errors.is_empty());
        assert_eq!(parsed.servers.len(), 3);

        let local = find(&parsed, "filesystem");
        assert_eq!(local.connection_type, "stdio");
        assert_eq!(local.command.as_deref(), Some("npx"));
        assert_eq!(
            local.args,
            Some(vec![
                "-y".to_string(),
                "@modelcontextprotocol/server-filesystem".to_string(),
                "/tmp".to_string()
            ])
        );

        let remote = find(&parsed, "linear");
        assert_eq!(remote.connection_type, "http");
        assert_eq!(remote.url.as_deref(), Some("https://mcp.linear.app/mcp"));

        let sse = find(&parsed, "legacy-events");
        assert_eq!(sse.connection_type, "sse");
        assert_eq!(sse.url.as_deref(), Some("https://mcp.example.com/sse"));
    }
}
