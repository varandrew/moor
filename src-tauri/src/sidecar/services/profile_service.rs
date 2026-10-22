//! Profile 领域服务。
//!
//! 封装 Profile 持久化和领域事件发射,让路由层只负责 HTTP 形状。

use crate::sidecar::db::profile_repo::{
    Profile, ProfileDetailServer, ProfileRepository, ProfileServerState, ProfileServerUpsertInput,
    ProfileUpdateResult, RemoveResult,
};
use crate::sidecar::db::server_repo::ServerRepository;
use crate::sidecar::db::Database;
use crate::sidecar::services::event_bus::{EventBus, Evt};
use crate::sidecar::services::tool_catalog::{ProfileToolGroup, ToolCatalogService};
use std::collections::HashSet;
use std::sync::Arc;

pub struct ProfileService;

/// profile 操作可能产生的领域错误。与 ServerServiceError 对齐,
/// 让路由层能把 NotFound 等映射到正确的 HTTP 状态码。
#[derive(Debug)]
pub enum ProfileServiceError {
    NotFound(String),
    Validation(String),
    Active(String),
    Internal(String),
}

/// 单一映射点:ProfileServiceError → AppError。
impl From<ProfileServiceError> for crate::sidecar::http::app_error::AppError {
    fn from(e: ProfileServiceError) -> Self {
        match e {
            ProfileServiceError::NotFound(m) => Self::not_found(m),
            ProfileServiceError::Validation(m) => Self::validation(m),
            ProfileServiceError::Active(m) => Self::active_profile(m),
            ProfileServiceError::Internal(m) => Self::internal(m),
        }
    }
}

impl ProfileService {
    pub fn list(db: &Database) -> Result<Vec<Profile>, ProfileServiceError> {
        ProfileRepository::new(db)
            .find_all()
            .map_err(ProfileServiceError::Internal)
    }

    pub fn create(db: &Database, name: &str) -> Result<Profile, ProfileServiceError> {
        if name.is_empty() {
            return Err(ProfileServiceError::Validation("name is required".into()));
        }
        ProfileRepository::new(db)
            .create(name)
            .map_err(ProfileServiceError::Internal)
    }

    pub fn get_detail(
        db: &Database,
        id: &str,
    ) -> Result<(Profile, Vec<ProfileDetailServer>), ProfileServiceError> {
        let repo = ProfileRepository::new(db);
        let profile = repo
            .find_by_id(id)
            .map_err(ProfileServiceError::Internal)?
            .ok_or_else(|| ProfileServiceError::NotFound("Profile not found".into()))?;
        let servers = repo
            .find_profile_servers(id)
            .map_err(ProfileServiceError::Internal)?;
        Ok((profile, servers))
    }

    pub fn update(
        db: &Database,
        event_bus: &EventBus,
        id: &str,
        update: crate::sidecar::db::profile_repo::ProfileUpdate<'_>,
    ) -> Result<Profile, ProfileServiceError> {
        if let Some(mode) = update.exposure_mode {
            if !matches!(mode, "direct" | "curated") {
                return Err(ProfileServiceError::Validation(
                    "exposureMode must be direct or curated".into(),
                ));
            }
        }
        if let Some(policy) = update.risk_policy {
            if !matches!(policy, "advisory" | "block_destructive") {
                return Err(ProfileServiceError::Validation(
                    "riskPolicy must be advisory or block_destructive".into(),
                ));
            }
        }
        if update.initial_pins.is_some() && update.exposure_mode != Some("curated") {
            return Err(ProfileServiceError::Validation(
                "initialPins requires curated exposureMode".into(),
            ));
        }
        let touches_exposure = update.exposure_mode.is_some() || update.risk_policy.is_some();
        let profile = match ProfileRepository::new(db)
            .update(id, update)
            .map_err(ProfileServiceError::Internal)?
        {
            ProfileUpdateResult::Updated(profile) => profile,
            ProfileUpdateResult::NotFound => {
                return Err(ProfileServiceError::NotFound("Profile not found".into()))
            }
            ProfileUpdateResult::ServerNotInProfile(id) => {
                return Err(ProfileServiceError::Validation(format!(
                    "Server {id} not found in profile"
                )))
            }
        };
        if touches_exposure {
            event_bus.emit(Evt::ToolsChanged {
                reason: "profile".into(),
            });
        }
        Ok(profile)
    }

    pub fn remove(db: &Database, id: &str) -> Result<(), ProfileServiceError> {
        match ProfileRepository::new(db)
            .remove(id)
            .map_err(ProfileServiceError::Internal)?
        {
            RemoveResult::Success => Ok(()),
            RemoveResult::NotFound => {
                Err(ProfileServiceError::NotFound("Profile not found".into()))
            }
            RemoveResult::Active => Err(ProfileServiceError::Active(
                "Cannot delete active profile".into(),
            )),
        }
    }

    /// 激活 profile 并发出 `profile:activated` 事件。
    /// 事件发射是领域规则——切了活动 profile,相关缓存(profiles/servers/logs)该失效——
    /// 所以它属于 service,不属于路由层。
    pub fn activate(
        db: &Database,
        event_bus: &Arc<EventBus>,
        id: &str,
    ) -> Result<Profile, ProfileServiceError> {
        let profile = ProfileRepository::new(db)
            .activate(id)
            .map_err(ProfileServiceError::Internal)?
            .ok_or_else(|| ProfileServiceError::NotFound("Profile not found".into()))?;
        event_bus.emit(Evt::ProfileActivated {
            profile_id: id.to_string(),
        });
        Ok(profile)
    }

    pub fn get_profile_server(
        db: &Database,
        profile_id: &str,
        server_id: &str,
    ) -> Result<ProfileDetailServer, ProfileServiceError> {
        let servers = ProfileRepository::new(db)
            .find_profile_servers(profile_id)
            .map_err(ProfileServiceError::Internal)?;
        servers
            .into_iter()
            .find(|s| s.server.id == server_id)
            .ok_or_else(|| ProfileServiceError::NotFound("Server not found in profile".into()))
    }

    /// 校验 server 存在：FK 约束失败会以原始 500 暴露，这里提前拦成 Validation（400）。
    fn ensure_servers_exist(
        db: &Database,
        server_ids: &[String],
    ) -> Result<(), ProfileServiceError> {
        let known: HashSet<String> = ServerRepository::new(db)
            .find_by_ids(server_ids)
            .map_err(ProfileServiceError::Internal)?
            .into_iter()
            .map(|server| server.id)
            .collect();
        let missing: Vec<&str> = server_ids
            .iter()
            .filter(|id| !known.contains(*id))
            .map(|id| id.as_str())
            .collect();
        if !missing.is_empty() {
            return Err(ProfileServiceError::Validation(format!(
                "unknown server ids: {missing:?}"
            )));
        }
        Ok(())
    }

    pub fn upsert_profile_server(
        db: &Database,
        profile_id: &str,
        server_id: &str,
        enabled: Option<bool>,
        disabled_tools: Option<&Vec<String>>,
    ) -> Result<ProfileServerState, ProfileServiceError> {
        Self::ensure_servers_exist(db, &[server_id.to_string()])?;
        ProfileRepository::new(db)
            .upsert_profile_server(profile_id, server_id, enabled, disabled_tools)
            .map_err(ProfileServiceError::Internal)
    }

    /// 批量更新 profile-server 状态。profile 不存在返回 NotFound（404 优先于写入），
    /// server 不存在返回 Validation（400），不让 FK 约束以 500 暴露。
    pub fn upsert_profile_servers_batch(
        db: &Database,
        profile_id: &str,
        items: &[ProfileServerUpsertInput],
    ) -> Result<(), ProfileServiceError> {
        let repo = ProfileRepository::new(db);
        repo.find_by_id(profile_id)
            .map_err(ProfileServiceError::Internal)?
            .ok_or_else(|| ProfileServiceError::NotFound("Profile not found".into()))?;
        if items.is_empty() {
            return Ok(());
        }
        let server_ids: Vec<String> = items.iter().map(|item| item.server_id.clone()).collect();
        Self::ensure_servers_exist(db, &server_ids)?;
        repo.upsert_profile_servers_batch(profile_id, items)
            .map_err(ProfileServiceError::Internal)
    }

    /// 克隆 profile（含 server 绑定与工具禁用态），新 profile 非激活。
    pub fn clone_profile(
        db: &Database,
        source_id: &str,
        name: &str,
    ) -> Result<Profile, ProfileServiceError> {
        if name.is_empty() {
            return Err(ProfileServiceError::Validation("name is required".into()));
        }
        ProfileRepository::new(db)
            .clone_profile(source_id, name)
            .map_err(ProfileServiceError::Internal)?
            .ok_or_else(|| ProfileServiceError::NotFound("Profile not found".into()))
    }

    /// 治理面板的分组工具矩阵。profile 不存在返回 NotFound。
    pub fn get_profile_tool_groups(
        db: &Database,
        profile_id: &str,
    ) -> Result<Vec<ProfileToolGroup>, ProfileServiceError> {
        ProfileRepository::new(db)
            .find_by_id(profile_id)
            .map_err(ProfileServiceError::Internal)?
            .ok_or_else(|| ProfileServiceError::NotFound("Profile not found".into()))?;
        Ok(ToolCatalogService::get_profile_tool_groups(db, profile_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sidecar::db::server_repo::{ServerInsertInput, ServerRepository};
    use crate::sidecar::db::tool_discovery_repo::{ToolDiscoveryRepository, ToolInsert};
    use crate::sidecar::services::tool_catalog::ToolCatalogService;
    use axum::http::StatusCode;
    use std::time::SystemTime;

    #[test]
    fn validation_error_maps_to_bad_request() {
        let err: crate::sidecar::http::app_error::AppError =
            ProfileServiceError::Validation("name is required".into()).into();

        assert_eq!(err.status_code(), StatusCode::BAD_REQUEST);
        assert_eq!(err.code(), "VALIDATION_ERROR");
    }

    // Hot-Swap 契约：激活另一个 profile 后，下一次目录解析立即反映其可见 Tools，并发出事件
    #[test]
    fn activate_hot_swaps_visible_tool_catalog_and_emits_event() {
        let ts = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("moor-profile-hot-swap-{ts}.db"));
        let db = Database::open(&path).expect("open db");
        db.run_migrations().expect("migrate");
        let profile_repo = ProfileRepository::new(&db);
        profile_repo.seed_default().expect("seed default profile");

        ServerRepository::new(&db)
            .insert_one_with_id(
                "server-a",
                0,
                &ServerInsertInput {
                    name: "Alpha".into(),
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
            .expect("insert server");
        ToolDiscoveryRepository::new(&db)
            .replace_tools_for_server(
                "server-a",
                &[ToolInsert {
                    name: "search".to_string(),
                    description: None,
                    input_schema: None,
                }],
            )
            .expect("insert tool");
        profile_repo
            .assign_to_active_profile(&["server-a".to_string()])
            .expect("assign to default profile");

        let focused = ProfileService::create(&db, "Focused").expect("create profile");
        ProfileRepository::new(&db)
            .upsert_profile_server(
                &focused.id,
                "server-a",
                Some(true),
                Some(&vec!["search".to_string()]),
            )
            .expect("deny tool in focused profile");

        let exposed: Vec<String> = ToolCatalogService::load_active_profile(&db)
            .unwrap()
            .resolve(None)
            .listed_tools()
            .into_iter()
            .map(|entry| entry.exposed_name)
            .collect();
        assert_eq!(exposed, vec!["alpha__search".to_string()]);

        let event_bus = Arc::new(EventBus::new(8));
        let mut events = event_bus.subscribe();
        ProfileService::activate(&db, &event_bus, &focused.id).expect("activate focused");

        let exposed: Vec<String> = ToolCatalogService::load_active_profile(&db)
            .unwrap()
            .resolve(None)
            .listed_tools()
            .into_iter()
            .map(|entry| entry.exposed_name)
            .collect();
        assert!(
            exposed.is_empty(),
            "Hot-Swap: catalog should reflect the new active profile, got {exposed:?}"
        );

        match events.try_recv() {
            Ok(Evt::ProfileActivated { profile_id }) => assert_eq!(profile_id, focused.id),
            other => panic!("expected ProfileActivated event, got {other:?}"),
        }

        let _ = std::fs::remove_file(path);
    }
}
