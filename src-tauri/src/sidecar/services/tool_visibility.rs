//! 工具可见性判定。
//!
//! Profile 的曝光模式与启用状态、Server 的运行状态、工具禁用清单与置顶选择，
//! 在此合并成唯一判定点。`tools/list`、`tools/call`、治理面板与 Server 详情的
//! 工具列表都从这里取结论，避免同一套规则在多处各写一遍。

use crate::sidecar::db::tool_discovery_repo::ProfileTool;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Profile 的曝光模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExposureMode {
    /// 全量直出：Profile 内所有可用 Tool 都出现在 `tools/list`。
    Direct,
    /// 策展：只有置顶 Tool 出现在 `tools/list`，其余仍可直接调用。
    Curated,
}

impl ExposureMode {
    pub fn from_db(value: &str) -> Self {
        match value {
            "curated" => Self::Curated,
            _ => Self::Direct,
        }
    }
}

/// 一个 Tool 在当前 Profile 下对 Agent 的可达状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolAccess {
    /// 可调用：`tools/call` 能解析到归属。是否出现在 `tools/list` 由 [`ToolVisibility::listed`] 决定。
    Callable,
    /// 在 Profile 的禁用清单中。
    ToolDisabled,
    /// 所属 Server 在 Profile 内未启用。
    ServerDisabled,
    /// 所属 Server 已启用但当前不可调用（未运行）。
    ServerUnavailable,
}

impl ToolAccess {
    pub fn is_callable(self) -> bool {
        matches!(self, Self::Callable)
    }

    /// 是否被 Profile 的禁用清单移除。与是否可达无关：Server 停用时禁用状态仍然保留。
    pub fn is_deny_listed(self) -> bool {
        matches!(self, Self::ToolDisabled)
    }
}

/// 单个 Tool 的判定结果。
#[derive(Debug, Clone)]
pub struct ToolVisibility {
    pub server_id: String,
    pub server_name: String,
    pub tool_name: String,
    pub description: Option<String>,
    pub input_schema: Option<serde_json::Value>,
    pub access: ToolAccess,
    /// 是否被当前 Profile 置顶。置顶与曝光模式无关，是用户的显式选择。
    pub pinned: bool,
    /// 是否出现在 `tools/list`。策展模式下未置顶的工具不可见但可调用。
    pub listed: bool,
    /// 对外暴露名。只有可调用的 Tool 才有名字——消歧只在可调用集合内计算。
    pub exposed_name: Option<String>,
}

/// 判定 Profile 内每个 Tool 的可达状态。
///
/// `callable_server_ids` 为 `None` 时忽略运行状态（治理面板、Server 详情需要看到未运行的 Server）。
pub fn resolve_tool_visibility(
    profile_tools: &[ProfileTool],
    callable_server_ids: Option<&HashSet<String>>,
    exposure_mode: ExposureMode,
) -> Vec<ToolVisibility> {
    let classified: Vec<(ToolAccess, &ProfileTool)> = profile_tools
        .iter()
        .map(|tool| (classify(tool, callable_server_ids), tool))
        .collect();

    // 消歧只在可调用集合内计算：名字不随置顶切换而变，被隐藏的工具也不占用名字。
    let mut server_ids_by_base_name = std::collections::HashMap::<String, Vec<String>>::new();
    for (access, tool) in &classified {
        if access.is_callable() {
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
            pinned: tool.pinned,
            listed: access.is_callable()
                && (matches!(exposure_mode, ExposureMode::Direct) || tool.pinned),
            exposed_name: access
                .is_callable()
                .then(|| exposed_name_for(tool, &server_ids_by_base_name)),
            access,
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
        _ => ToolAccess::Callable,
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
        .expect("callable tools always register their base name");
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
        pinned: bool,
        tool_name: &str,
    ) -> ProfileTool {
        ProfileTool {
            server_id: server_id.to_string(),
            server_name: server_name.to_string(),
            enabled,
            disabled_tools: disabled.iter().map(|name| name.to_string()).collect(),
            pinned,
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

    fn listed_of(result: &[ToolVisibility], server_id: &str, tool_name: &str) -> bool {
        result
            .iter()
            .find(|item| item.server_id == server_id && item.tool_name == tool_name)
            .expect("tool should be present in the resolution")
            .listed
    }

    #[test]
    fn classifies_each_reason_a_tool_is_not_exposed() {
        // (Server 启用, 工具在禁用清单, Server 可调用, 期望状态)
        let cases = [
            (true, false, true, ToolAccess::Callable),
            (true, true, true, ToolAccess::ToolDisabled),
            (false, false, true, ToolAccess::ServerDisabled),
            (true, false, false, ToolAccess::ServerUnavailable),
            (false, true, true, ToolAccess::ToolDisabled),
        ];

        for (server_enabled, denied, callable_server, expected) in cases {
            let disabled: &[&str] = if denied { &["search"] } else { &[] };
            let rows = vec![tool(
                "s1",
                "Alpha",
                server_enabled,
                disabled,
                false,
                "search",
            )];
            let ids = if callable_server {
                Some(callable(&["s1"]))
            } else {
                Some(callable(&[]))
            };

            let result = resolve_tool_visibility(&rows, ids.as_ref(), ExposureMode::Direct);

            assert_eq!(
                access_of(&result, "s1", "search"),
                expected,
                "enabled={server_enabled} denied={denied} callable={callable_server}"
            );
        }
    }

    #[test]
    fn only_callable_tools_get_an_exposed_name() {
        let rows = vec![
            tool("s1", "Alpha", true, &[], false, "search"),
            tool("s1", "Alpha", true, &["hidden"], false, "hidden"),
            tool("s2", "Beta", false, &[], false, "read"),
            tool("s3", "Gamma", true, &[], false, "write"),
        ];
        let ids = callable(&["s1", "s2"]);

        let result = resolve_tool_visibility(&rows, Some(&ids), ExposureMode::Direct);
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
        let rows = vec![tool("s3", "Gamma", true, &[], false, "write")];

        let result = resolve_tool_visibility(&rows, None, ExposureMode::Direct);

        assert_eq!(result[0].access, ToolAccess::Callable);
        assert_eq!(result[0].exposed_name.as_deref(), Some("gamma__write"));
    }

    #[test]
    fn keeps_the_deny_list_visible_for_a_disabled_server() {
        let rows = vec![tool("s2", "Beta", false, &["read"], false, "read")];

        let result = resolve_tool_visibility(&rows, Some(&callable(&["s2"])), ExposureMode::Direct);

        assert_eq!(
            result[0].access,
            ToolAccess::ToolDisabled,
            "the deny list survives a disabled server"
        );
        assert!(result[0].access.is_deny_listed());
    }

    #[test]
    fn disambiguates_names_only_among_callable_tools() {
        let rows = vec![
            tool("aaaaaaaa1111", "GitHub MCP", true, &[], false, "search"),
            tool("aaaaaaaa2222", "github-mcp", true, &[], false, "search"),
        ];

        let without_deny = resolve_tool_visibility(&rows, None, ExposureMode::Direct);
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
        let with_deny = resolve_tool_visibility(&denied_rows, None, ExposureMode::Direct);
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
    fn lists_every_callable_tool_in_direct_mode() {
        let rows = vec![
            tool("s1", "Alpha", true, &[], false, "search"),
            tool("s1", "Alpha", true, &[], true, "pinned"),
        ];

        let result = resolve_tool_visibility(&rows, None, ExposureMode::Direct);

        assert!(listed_of(&result, "s1", "search"));
        assert!(listed_of(&result, "s1", "pinned"));
    }

    #[test]
    fn lists_only_pinned_tools_in_curated_mode() {
        let rows = vec![
            tool("s1", "Alpha", true, &[], false, "search"),
            tool("s1", "Alpha", true, &[], true, "pinned"),
        ];

        let result = resolve_tool_visibility(&rows, None, ExposureMode::Curated);

        assert!(!listed_of(&result, "s1", "search"));
        assert!(listed_of(&result, "s1", "pinned"));
        assert_eq!(
            access_of(&result, "s1", "search"),
            ToolAccess::Callable,
            "an unpinned tool stays callable so cached names keep working"
        );
        let hidden = result
            .iter()
            .find(|item| item.tool_name == "search")
            .expect("tool should be resolved");
        assert_eq!(
            hidden.exposed_name.as_deref(),
            Some("alpha__search"),
            "an unpinned tool still has a name to be called by"
        );
    }

    #[test]
    fn a_denied_tool_is_never_listed_even_when_pinned() {
        let rows = vec![tool("s1", "Alpha", true, &["search"], true, "search")];

        let result = resolve_tool_visibility(&rows, None, ExposureMode::Curated);

        assert!(!listed_of(&result, "s1", "search"));
        assert_eq!(access_of(&result, "s1", "search"), ToolAccess::ToolDisabled);
    }

    #[test]
    fn names_do_not_depend_on_the_exposure_mode() {
        let rows = vec![tool("s1", "Alpha", true, &[], false, "search")];

        let direct = resolve_tool_visibility(&rows, None, ExposureMode::Direct);
        let curated = resolve_tool_visibility(&rows, None, ExposureMode::Curated);

        assert_eq!(direct[0].exposed_name, curated[0].exposed_name);
    }

    #[test]
    fn exposes_a_tool_when_its_server_is_callable_and_not_denied() {
        let rows = vec![tool("s1", "Alpha", true, &["other"], false, "search")];

        let result = resolve_tool_visibility(&rows, Some(&callable(&["s1"])), ExposureMode::Direct);

        assert!(result[0].access.is_callable());
        assert!(!result[0].access.is_deny_listed());
    }
}
