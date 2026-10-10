use crate::sidecar::config::import_parser::ScannedServer;
use crate::sidecar::config::scanner;
use crate::sidecar::db::Database;
use crate::sidecar::services::server_manager::ServerManager;
use crate::sidecar::services::server_service::{CreateServerInput, ServerService};
use std::sync::Arc;

pub struct ImportResult {
    pub imported: Vec<String>,
    pub skipped: Vec<String>,
}

pub async fn execute_import(
    db: &Arc<Database>,
    server_manager: &Arc<ServerManager>,
    servers: Option<Vec<ScannedServer>>,
) -> Result<ImportResult, String> {
    let existing_names = ServerService::find_all_names(db);
    let all_servers = match servers {
        Some(s) if !s.is_empty() => s,
        _ => scanner::scan_all_configs().servers,
    };
    let (candidates, skipped) = scanner::partition_import_candidates(&all_servers, &existing_names);

    let inputs: Vec<CreateServerInput> = candidates
        .iter()
        .map(|sc| CreateServerInput {
            name: sc.name.clone(),
            // SSE 导入沿用 HTTP 存储，由传输层识别协议。
            connection_type: if sc.connection_type == "sse" {
                "http".to_string()
            } else {
                sc.connection_type.clone()
            },
            command: sc.command.clone(),
            args: sc.args.clone(),
            url: sc.url.clone(),
            env: sc.env.clone(),
            headers: sc.headers.clone(),
            working_dir: sc.working_dir.clone(),
            auto_start: false,
        })
        .collect();

    let imported_servers = ServerService::insert_servers(db, server_manager, &inputs).await?;
    let imported = imported_servers.into_iter().map(|s| s.name).collect();

    Ok(ImportResult { imported, skipped })
}
