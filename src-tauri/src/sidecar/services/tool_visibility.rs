//! 工具可见性判定。
//!
//! Profile 的启用状态、Server 的运行状态与工具禁用清单在此合并成唯一判定点。
//! `tools/list`、`tools/call`、治理面板与 Server 详情的工具列表都从这里取结论，
//! 避免同一套规则在多处各写一遍。

use crate::sidecar::db::tool_discovery_repo::ProfileTool;
use std::collections::HashSet;

/// 一个 Tool 在当前 Profile 下对 Agent 的可达状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolAccess {
    /// 直出：出现在 `tools/list`，且 `tools/call` 能解析到归属。
    Exposed,
    /// 在 Profile 的禁用清单中。
    ToolDisabled,
    /// 所属 Server 在 Profile 内未启用。
    ServerDisabled,
    /// 所属 Server 已启用但当前不可调用（未运行）。
    ServerUnavailable,
}

impl ToolAccess {
    pub fn is_exposed(self) -> bool {
        matches!(self, Self::Exposed)
    }

    /// 是否被 Profile 的禁用清单移除。与是否可达无关：Server 停用时禁用状态仍然保留。
    pub fn is_deny_listed(self) -> bool {
        matches!(self, Self::ToolDisabled)
    }
}

/// 单个 Tool 的判定结果。
///
/// `exposed_name` 只在 [`ToolAccess::Exposed`] 时为 `Some`：暴露名的消歧只在可见集合内计算，
/// 不可见的工具没有对外的名字。
#[derive(Debug, Clone)]
pub struct ToolVisibility {
    pub server_id: String,
    pub server_name: String,
    pub tool_name: String,
    pub description: Option<String>,
    pub input_schema: Option<serde_json::Value>,
    pub access: ToolAccess,
    pub exposed_name: Option<String>,
}

/// 判定 Profile 内每个 Tool 的可达状态。
///
/// `callable_server_ids` 为 `None` 时忽略运行状态（治理面板、Server 详情需要看到未运行的 Server）。
pub fn resolve_tool_visibility(
    profile_tools: &[ProfileTool],
    callable_server_ids: Option<&HashSet<String>>,
) -> Vec<ToolVisibility> {
    let classified: Vec<(ToolAccess, &ProfileTool)> = profile_tools
        .iter()
        .map(|tool| (classify(tool, callable_server_ids), tool))
        .collect();

    // 消歧只在可见集合内计算：被隐藏的工具不占用名字，也不影响别人的名字。
    let mut server_ids_by_base_name = std::collections::HashMap::<String, Vec<String>>::new();
    for (access, tool) in &classified {
        if access.is_exposed() {
            server_ids_by_base_name
                .entry(base_name(tool))
                .or_default()
                .push(tool.server_id.clone());
        }
    }

    classified
        .into_iter()
        .map(|(access, tool)| ToolVisibility {
            server_id: tool.server_id.clone(),
            server_name: tool.server_name.clone(),
            tool_name: tool.tool_name.clone(),
            description: tool.description.clone(),
            input_schema: tool.input_schema.clone(),
            access,
            exposed_name: access
                .is_exposed()
                .then(|| exposed_name_for(tool, &server_ids_by_base_name)),
        })
        .collect()
}

fn classify(tool: &ProfileTool, callable_server_ids: Option<&HashSet<String>>) -> ToolAccess {
    // 禁用清单先判：Server 停用后工具仍应显示为「已禁用」，而不是丢掉这层状态。
    if tool
        .disabled_tools
        .iter()
        .any(|name| *name == tool.tool_name)
    {
        return ToolAccess::ToolDisabled;
    }
    if !tool.enabled {
        return ToolAccess::ServerDisabled;
    }
    match callable_server_ids {
        Some(ids) if !ids.contains(&tool.server_id) => ToolAccess::ServerUnavailable,
        _ => ToolAccess::Exposed,
    }
}

fn base_name(tool: &ProfileTool) -> String {
    format!(
        "{}__{}",
        normalize_server_name(&tool.server_name),
        tool.tool_name
    )
}

fn exposed_name_for(
    tool: &ProfileTool,
    server_ids_by_base_name: &std::collections::HashMap<String, Vec<String>>,
) -> String {
    let base = base_name(tool);
    let server_ids = server_ids_by_base_name
        .get(&base)
        .expect("exposed tools always register their base name");
    if server_ids.len() > 1 {
        format!(
            "{}_{}__{}",
            normalize_server_name(&tool.server_name),
            shortest_unique_server_id_prefix(&tool.server_id, server_ids),
            tool.tool_name
        )
    } else {
        base
    }
}

pub(crate) fn normalize_server_name(name: &str) -> String {
    let slug: String = name
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let slug = slug.trim_matches('_');
    if slug.is_empty() {
        "server".to_string()
    } else {
        slug.to_string()
    }
}

fn shortest_unique_server_id_prefix(server_id: &str, server_ids: &[String]) -> String {
    for length in std::cmp::min(8, server_id.len())..=server_id.len() {
        let prefix = &server_id[..length];
        if server_ids
            .iter()
            .all(|candidate| candidate == server_id || !candidate.starts_with(prefix))
        {
            return prefix.to_string();
        }
    }
    server_id.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(
        server_id: &str,
        server_name: &str,
        enabled: bool,
        disabled: &[&str],
        tool_name: &str,
    ) -> ProfileTool {
        ProfileTool {
            server_id: server_id.to_string(),
            server_name: server_name.to_string(),
            enabled,
            disabled_tools: disabled.iter().map(|name| name.to_string()).collect(),
            tool_name: tool_name.to_string(),
            description: None,
            input_schema: None,
        }
    }

    fn callable(ids: &[&str]) -> HashSet<String> {
        ids.iter().map(|id| id.to_string()).collect()
    }

    fn access_of(result: &[ToolVisibility], server_id: &str, tool_name: &str) -> ToolAccess {
        result
            .iter()
            .find(|item| item.server_id == server_id && item.tool_name == tool_name)
            .expect("tool should be present in the resolution")
            .access
    }

    #[test]
    fn classifies_each_reason_a_tool_is_not_exposed() {
        // (Server 启用, 工具在禁用清单, Server 可调用, 期望状态)
        let cases = [
            (true, false, true, ToolAccess::Exposed),
            (true, true, true, ToolAccess::ToolDisabled),
            (false, false, true, ToolAccess::ServerDisabled),
            (true, false, false, ToolAccess::ServerUnavailable),
            (false, true, true, ToolAccess::ToolDisabled),
        ];

        for (server_enabled, denied, callable_server, expected) in cases {
            let disabled: &[&str] = if denied { &["search"] } else { &[] };
            let rows = vec![tool("s1", "Alpha", server_enabled, disabled, "search")];
            let ids = if callable_server {
                Some(callable(&["s1"]))
            } else {
                Some(callable(&[]))
            };

            let result = resolve_tool_visibility(&rows, ids.as_ref());

            assert_eq!(
                access_of(&result, "s1", "search"),
                expected,
                "enabled={server_enabled} denied={denied} callable={callable_server}"
            );
        }
    }

    #[test]
    fn only_exposed_tools_get_an_exposed_name() {
        let rows = vec![
            tool("s1", "Alpha", true, &[], "search"),
            tool("s1", "Alpha", true, &["hidden"], "hidden"),
            tool("s2", "Beta", false, &[], "read"),
            tool("s3", "Gamma", true, &[], "write"),
        ];
        let ids = callable(&["s1", "s2"]);

        let result = resolve_tool_visibility(&rows, Some(&ids));
        let name_of = |server_id: &str, tool_name: &str| {
            result
                .iter()
                .find(|item| item.server_id == server_id && item.tool_name == tool_name)
                .and_then(|item| item.exposed_name.clone())
        };

        assert_eq!(name_of("s1", "search").as_deref(), Some("alpha__search"));
        assert_eq!(name_of("s1", "hidden"), None);
        assert_eq!(name_of("s2", "read"), None);
        assert_eq!(name_of("s3", "write"), None);
    }

    #[test]
    fn ignores_runtime_state_when_no_callable_set_is_given() {
        let rows = vec![tool("s3", "Gamma", true, &[], "write")];

        let result = resolve_tool_visibility(&rows, None);

        assert_eq!(result[0].access, ToolAccess::Exposed);
        assert_eq!(result[0].exposed_name.as_deref(), Some("gamma__write"));
    }

    #[test]
    fn keeps_the_deny_list_visible_for_a_disabled_server() {
        let rows = vec![tool("s2", "Beta", false, &["read"], "read")];

        let result = resolve_tool_visibility(&rows, Some(&callable(&["s2"])));

        assert_eq!(
            result[0].access,
            ToolAccess::ToolDisabled,
            "the deny list survives a disabled server"
        );
        assert!(result[0].access.is_deny_listed());
    }

    #[test]
    fn disambiguates_names_only_among_exposed_tools() {
        let rows = vec![
            tool("aaaaaaaa1111", "GitHub MCP", true, &[], "search"),
            tool("aaaaaaaa2222", "github-mcp", true, &[], "search"),
        ];

        let without_deny = resolve_tool_visibility(&rows, None);
        let exposed: Vec<_> = without_deny
            .iter()
            .filter_map(|item| item.exposed_name.clone())
            .collect();
        assert_eq!(
            exposed,
            vec![
                "github_mcp_aaaaaaaa1__search".to_string(),
                "github_mcp_aaaaaaaa2__search".to_string()
            ]
        );

        let mut denied_rows = rows.clone();
        denied_rows[1].disabled_tools = vec!["search".to_string()];
        let with_deny = resolve_tool_visibility(&denied_rows, None);
        let exposed_after_deny: Vec<_> = with_deny
            .iter()
            .filter_map(|item| item.exposed_name.clone())
            .collect();
        assert_eq!(
            exposed_after_deny,
            vec!["github_mcp__search".to_string()],
            "a denied tool frees its base name for the survivor"
        );
    }

    #[test]
    fn exposes_a_tool_when_its_server_is_callable_and_not_denied() {
        let rows = vec![tool("s1", "Alpha", true, &["other"], "search")];

        let result = resolve_tool_visibility(&rows, Some(&callable(&["s1"])));

        assert!(result[0].access.is_exposed());
        assert!(!result[0].access.is_deny_listed());
    }
}
