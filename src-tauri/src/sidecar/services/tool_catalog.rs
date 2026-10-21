use crate::sidecar::db::profile_repo::ProfileRepository;
use crate::sidecar::db::tool_discovery_repo::{
    ProfileTool, ToolDiscovery, ToolDiscoveryRepository,
};
use crate::sidecar::db::Database;
use crate::sidecar::services::tool_risk::{RiskLevel, RiskPolicy};
use crate::sidecar::services::tool_visibility::{
    normalize_server_name, resolve_tool_visibility, ExposureMode, ToolVisibility,
};
use serde::Serialize;
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct ToolCatalogEntry {
    pub server_name: String,
    pub risk_level: String,
    pub tool_name: String,
    pub exposed_name: String,
    pub description: Option<String>,
    pub input_schema: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDetail {
    pub tool_name: String,
    pub exposed_name: String,
    pub description: Option<String>,
    pub input_schema: Option<serde_json::Value>,
    pub disabled: bool,
    pub pinned: bool,
    pub risk_level: String,
    pub risk_source: String,
}

pub struct ToolCatalogService;

impl ToolCatalogService {
    pub fn get_tool_catalog(
        db: &Database,
        profile_id: Option<&str>,
        visible_server_ids: Option<&std::collections::HashSet<String>>,
    ) -> Vec<ToolCatalogEntry> {
        let profile_repo = ProfileRepository::new(db);
        let active_id = match profile_id {
            Some(id) => id.to_string(),
            None => match profile_repo.find_active_id() {
                Ok(Some(id)) => id,
                _ => return vec![],
            },
        };

        let tool_repo = ToolDiscoveryRepository::new(db);
        let profile_tools = match tool_repo.find_by_profile_id(&active_id) {
            Ok(t) => t,
            Err(_) => return vec![],
        };

        let (mode, policy) = profile_exposure(db, &active_id);
        build_tool_catalog_entries(&profile_tools, visible_server_ids, mode, policy)
    }

    /// `tools/call` 的归属解析用：策展模式下未置顶的工具仍可被调用。
    pub fn get_callable_tool_catalog(
        db: &Database,
        profile_id: Option<&str>,
        visible_server_ids: Option<&std::collections::HashSet<String>>,
    ) -> Vec<ToolCatalogEntry> {
        let profile_repo = ProfileRepository::new(db);
        let active_id = match profile_id {
            Some(id) => id.to_string(),
            None => match profile_repo.find_active_id() {
                Ok(Some(id)) => id,
                _ => return vec![],
            },
        };

        let tool_repo = ToolDiscoveryRepository::new(db);
        let profile_tools = match tool_repo.find_by_profile_id(&active_id) {
            Ok(t) => t,
            Err(_) => return vec![],
        };

        let (_, policy) = profile_exposure(db, &active_id);
        build_callable_catalog_entries(&profile_tools, visible_server_ids, policy)
    }

    /// `tools/call` 的错误分类需要完整判定（含被策略拦下的工具），而不只是可调用集合。
    pub fn resolve_profile_tools(
        db: &Database,
        profile_id: Option<&str>,
        visible_server_ids: Option<&std::collections::HashSet<String>>,
    ) -> Vec<ToolVisibility> {
        let profile_repo = ProfileRepository::new(db);
        let active_id = match profile_id {
            Some(id) => id.to_string(),
            None => match profile_repo.find_active_id() {
                Ok(Some(id)) => id,
                _ => return vec![],
            },
        };
        let tool_repo = ToolDiscoveryRepository::new(db);
        let profile_tools = match tool_repo.find_by_profile_id(&active_id) {
            Ok(t) => t,
            Err(_) => return vec![],
        };
        let (mode, policy) = profile_exposure(db, &active_id);
        resolve_tool_visibility(&profile_tools, visible_server_ids, mode, policy)
    }

    pub fn get_tool_details(
        db: &Database,
        server_id: &str,
        profile_id: Option<&str>,
        visible_server_ids: Option<&std::collections::HashSet<String>>,
    ) -> Vec<ToolDetail> {
        let profile_repo = ProfileRepository::new(db);
        let active_id = match profile_id {
            Some(id) => id.to_string(),
            None => match profile_repo.find_active_id() {
                Ok(Some(id)) => id,
                _ => return vec![],
            },
        };

        let tool_repo = ToolDiscoveryRepository::new(db);
        let profile_tools = match tool_repo.find_by_profile_id(&active_id) {
            Ok(t) => t,
            Err(_) => return vec![],
        };

        let server_name = crate::sidecar::db::server_repo::ServerRepository::new(db)
            .find_by_id(server_id)
            .ok()
            .flatten()
            .map(|server| server.name)
            .unwrap_or_else(|| "server".to_string());

        let (mode, policy) = profile_exposure(db, &active_id);
        let visibility = visibility_by_server(resolve_tool_visibility(
            &profile_tools,
            visible_server_ids,
            mode,
            policy,
        ));
        let discovered = match tool_repo.find_by_server_id(server_id) {
            Ok(tools) => tools,
            Err(_) => return vec![],
        };

        let resolved = visibility.get(server_id);
        discovered
            .into_iter()
            .map(|tool| {
                let resolved = resolved.and_then(|by_tool| by_tool.get(&tool.tool_name));
                tool_detail(resolved, &server_name, tool)
            })
            .collect()
    }

    /// 治理面板数据：按 server 分组返回 profile 内全部已发现工具及禁用态。
    /// 批量读取工具并复用目录命名规则，保留禁用服务与工具的治理入口。
    /// 读取失败降级为空组——面板是只读视图，不应因单个 server 失败而整体 500。
    pub fn get_profile_tool_groups(db: &Database, profile_id: &str) -> Vec<ProfileToolGroup> {
        let servers = match ProfileRepository::new(db).find_profile_servers(profile_id) {
            Ok(servers) => servers,
            Err(_) => return vec![],
        };
        let tool_repo = ToolDiscoveryRepository::new(db);
        let profile_tools = match tool_repo.find_by_profile_id(profile_id) {
            Ok(tools) => tools,
            Err(_) => return vec![],
        };
        let (mode, policy) = profile_exposure(db, profile_id);
        let visibility =
            visibility_by_server(resolve_tool_visibility(&profile_tools, None, mode, policy));
        let mut discovered_by_server = std::collections::HashMap::<String, Vec<_>>::new();
        for tool in tool_repo.find_all().unwrap_or_default() {
            discovered_by_server
                .entry(tool.server_id.clone())
                .or_default()
                .push(tool);
        }
        servers
            .into_iter()
            .map(|server| {
                let resolved = visibility.get(&server.server.id);
                let tools = discovered_by_server
                    .remove(&server.server.id)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|tool| {
                        let resolved = resolved.and_then(|by_tool| by_tool.get(&tool.tool_name));
                        tool_detail(resolved, &server.server.name, tool)
                    })
                    .collect();
                ProfileToolGroup {
                    server_id: server.server.id,
                    server_name: server.server.name,
                    server_enabled: server.profile_server.enabled,
                    tools,
                }
            })
            .collect()
    }
}

/// 治理面板分组：profile 内全部 server 的已发现工具及其在该 profile 下的禁用态。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileToolGroup {
    pub server_id: String,
    pub server_name: String,
    pub server_enabled: bool,
    pub tools: Vec<ToolDetail>,
}

/// Profile 的两项曝光设置：曝光模式与风险策略。
fn profile_exposure(db: &Database, profile_id: &str) -> (ExposureMode, RiskPolicy) {
    ProfileRepository::new(db)
        .find_by_id(profile_id)
        .ok()
        .flatten()
        .map(|profile| {
            (
                ExposureMode::from_db(&profile.exposure_mode),
                RiskPolicy::from_db(&profile.risk_policy),
            )
        })
        .unwrap_or((ExposureMode::Direct, RiskPolicy::Advisory))
}

/// Agent 看到的目录：`tools/list` 的输入。
fn build_tool_catalog_entries(
    profile_tools: &[ProfileTool],
    callable_server_ids: Option<&HashSet<String>>,
    mode: ExposureMode,
    policy: RiskPolicy,
) -> Vec<ToolCatalogEntry> {
    catalog_entries(profile_tools, callable_server_ids, mode, policy, |item| {
        item.listed
    })
}

/// 可调用的完整集合：`tools/call` 的归属解析用，不受置顶影响。
fn build_callable_catalog_entries(
    profile_tools: &[ProfileTool],
    callable_server_ids: Option<&HashSet<String>>,
    policy: RiskPolicy,
) -> Vec<ToolCatalogEntry> {
    catalog_entries(
        profile_tools,
        callable_server_ids,
        ExposureMode::Direct,
        policy,
        |item| item.access.is_callable(),
    )
}

fn catalog_entries(
    profile_tools: &[ProfileTool],
    callable_server_ids: Option<&HashSet<String>>,
    mode: ExposureMode,
    policy: RiskPolicy,
    keep: impl Fn(&ToolVisibility) -> bool,
) -> Vec<ToolCatalogEntry> {
    resolve_tool_visibility(profile_tools, callable_server_ids, mode, policy)
        .into_iter()
        .filter(keep)
        .filter_map(|visible| {
            Some(ToolCatalogEntry {
                server_name: visible.server_name,
                risk_level: visible.risk.as_str().to_string(),
                tool_name: visible.tool_name,
                exposed_name: visible.exposed_name?,
                description: visible.description,
                input_schema: visible.input_schema,
            })
        })
        .collect()
}

/// 按 Server 归档判定结果，供逐工具查询；`None` 表示该工具不属于当前 Profile。
fn visibility_by_server(
    items: Vec<ToolVisibility>,
) -> std::collections::HashMap<String, std::collections::HashMap<String, ToolVisibility>> {
    let mut index = std::collections::HashMap::new();
    for item in items {
        index
            .entry(item.server_id.clone())
            .or_insert_with(std::collections::HashMap::new)
            .insert(item.tool_name.clone(), item);
    }
    index
}

/// 治理面板与 Server 详情共用：可见工具用判定的暴露名，其余退回基础名。
fn tool_detail(
    resolved: Option<&ToolVisibility>,
    server_name: &str,
    tool: ToolDiscovery,
) -> ToolDetail {
    let risk_level = resolved
        .map(|item| item.risk.as_str().to_string())
        .unwrap_or_else(|| RiskLevel::from_db(&tool.risk_level).as_str().to_string());
    ToolDetail {
        exposed_name: resolved
            .and_then(|item| item.exposed_name.clone())
            .unwrap_or_else(|| {
                format!("{}__{}", normalize_server_name(server_name), tool.tool_name)
            }),
        disabled: resolved.is_some_and(|item| item.access.is_deny_listed()),
        pinned: resolved.is_some_and(|item| item.pinned),
        risk_level,
        risk_source: tool.risk_source,
        tool_name: tool.tool_name,
        description: tool.description,
        input_schema: tool.input_schema,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sidecar::db::profile_repo::ProfileRepository;
    use crate::sidecar::db::server_repo::ServerRepository;
    use crate::sidecar::db::tool_discovery_repo::{ToolDiscoveryRepository, ToolInsert};
    use std::time::SystemTime;

    fn temp_db_path(test_name: &str) -> std::path::PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("system time is before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("moor-tool-catalog-{test_name}-{timestamp}.db"))
    }

    fn insert_server(db: &Database, id: &str, name: &str) {
        use crate::sidecar::db::server_repo::ServerInsertInput;
        ServerRepository::new(db)
            .insert_one_with_id(
                id,
                0,
                &ServerInsertInput {
                    name: name.into(),
                    connection_type: "stdio".into(),
                    command: Some("node".into()),
                    args: None,
                    url: None,
                    env: None,
                    headers: None,
                    working_dir: None,
                    auto_start: false,
                },
            )
            .expect("failed to insert server");
    }

    #[test]
    fn adds_short_unique_server_id_prefix_when_server_slugs_collide() {
        let db_path = temp_db_path("slug-collision");
        let db = Database::open(&db_path).expect("failed to open db");
        db.run_migrations().expect("failed to migrate");
        let profile_repo = ProfileRepository::new(&db);
        profile_repo.seed_default().expect("failed to seed profile");

        insert_server(&db, "aaaaaaaa1111", "GitHub MCP");
        insert_server(&db, "aaaaaaaa2222", "github-mcp");
        insert_server(&db, "aaaaaaaa3333", "github mcp");
        profile_repo
            .assign_to_active_profile(&[
                "aaaaaaaa1111".to_string(),
                "aaaaaaaa2222".to_string(),
                "aaaaaaaa3333".to_string(),
            ])
            .expect("failed to assign profile servers");

        let tool_repo = ToolDiscoveryRepository::new(&db);
        let search_tool = [ToolInsert {
            name: "search".to_string(),
            description: None,
            input_schema: None,
        }];
        tool_repo
            .replace_tools_for_server("aaaaaaaa1111", &search_tool)
            .expect("failed to insert tools for first server");
        tool_repo
            .replace_tools_for_server("aaaaaaaa2222", &search_tool)
            .expect("failed to insert tools for second server");
        tool_repo
            .replace_tools_for_server("aaaaaaaa3333", &search_tool)
            .expect("failed to insert tools for disabled server");
        let profile_id = profile_repo
            .find_active_id()
            .expect("failed to find active profile")
            .expect("active profile should exist");
        profile_repo
            .upsert_profile_server(
                &profile_id,
                "aaaaaaaa3333",
                Some(true),
                Some(&vec!["search".to_string()]),
            )
            .expect("failed to disable tool");

        let exposed_names: Vec<_> = ToolCatalogService::get_tool_catalog(&db, None, None)
            .into_iter()
            .map(|tool| tool.exposed_name)
            .collect();

        assert_eq!(
            exposed_names,
            vec![
                "github_mcp_aaaaaaaa1__search".to_string(),
                "github_mcp_aaaaaaaa2__search".to_string(),
            ]
        );

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn profile_groups_preserve_disabled_tools_and_catalog_names() {
        let db_path = temp_db_path("profile-groups");
        let db = Database::open(&db_path).expect("open db");
        db.run_migrations().expect("migrate");
        let profile_repo = ProfileRepository::new(&db);
        profile_repo.seed_default().expect("seed profile");
        for (id, name) in [
            ("aaaaaaaa1111", "Alpha"),
            ("aaaaaaaa2222", "Alpha"),
            ("disabled", "Disabled"),
            ("unassigned", "Unassigned"),
            ("empty", "Empty"),
        ] {
            insert_server(&db, id, name);
        }
        profile_repo
            .assign_to_active_profile(&[
                "aaaaaaaa1111".into(),
                "aaaaaaaa2222".into(),
                "disabled".into(),
                "empty".into(),
            ])
            .expect("assign servers");
        let profile_id = profile_repo.find_active_id().unwrap().unwrap();
        profile_repo
            .upsert_profile_server(&profile_id, "disabled", Some(false), None)
            .expect("disable server");
        profile_repo
            .upsert_profile_server(
                &profile_id,
                "aaaaaaaa2222",
                None,
                Some(&vec!["hidden".into()]),
            )
            .expect("disable tool");
        let tool_repo = ToolDiscoveryRepository::new(&db);
        for id in ["aaaaaaaa1111", "aaaaaaaa2222", "disabled", "unassigned"] {
            tool_repo
                .replace_tools_for_server(
                    id,
                    &["search", "hidden"].map(|name| ToolInsert {
                        name: name.into(),
                        description: Some(name.into()),
                        input_schema: Some(serde_json::json!({"type": "object"})),
                    }),
                )
                .expect("discover tools");
        }

        let groups = ToolCatalogService::get_profile_tool_groups(&db, &profile_id);
        assert_eq!(groups.len(), 5);
        for group in &groups {
            let expected = ToolCatalogService::get_tool_details(
                &db,
                &group.server_id,
                Some(&profile_id),
                None,
            );
            assert_eq!(
                serde_json::to_value(&group.tools).unwrap(),
                serde_json::to_value(expected).unwrap()
            );
            assert_eq!(
                group.server_enabled,
                !matches!(group.server_id.as_str(), "disabled" | "unassigned")
            );
        }
        let collision = groups
            .iter()
            .find(|group| group.server_id == "aaaaaaaa1111")
            .unwrap();
        assert!(collision
            .tools
            .iter()
            .any(|tool| tool.exposed_name == "alpha_aaaaaaaa1__search"));
        assert!(groups
            .iter()
            .find(|group| group.server_id == "empty")
            .unwrap()
            .tools
            .is_empty());
        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn keeps_server_slug_for_disabled_server_tool_details() {
        let db_path = temp_db_path("disabled-server-details");
        let db = Database::open(&db_path).expect("failed to open db");
        db.run_migrations().expect("failed to migrate");
        let profile_repo = ProfileRepository::new(&db);
        profile_repo.seed_default().expect("failed to seed profile");

        insert_server(&db, "server-a", "Alpha");
        profile_repo
            .assign_to_active_profile(&["server-a".to_string()])
            .expect("failed to assign profile server");
        let profile_id = profile_repo
            .find_active_id()
            .expect("failed to find active profile")
            .expect("active profile should exist");
        profile_repo
            .upsert_profile_server(&profile_id, "server-a", Some(false), None)
            .expect("failed to disable server");
        ToolDiscoveryRepository::new(&db)
            .replace_tools_for_server(
                "server-a",
                &[ToolInsert {
                    name: "search".to_string(),
                    description: None,
                    input_schema: None,
                }],
            )
            .expect("failed to insert tool");

        let exposed_names: Vec<_> =
            ToolCatalogService::get_tool_details(&db, "server-a", Some(&profile_id), None)
                .into_iter()
                .map(|tool| tool.exposed_name)
                .collect();

        assert_eq!(exposed_names, vec!["alpha__search".to_string()]);

        let _ = std::fs::remove_file(db_path);
    }
}
