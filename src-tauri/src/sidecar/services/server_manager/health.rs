use super::{ServerManager, ServerStatus};
use crate::sidecar::mcp::transport::request_error::RequestError;
use crate::sidecar::services::{
    event_bus::Evt, server_log::redact_sensitive_stderr_values, settings,
};
use futures::stream::{self, StreamExt};
use serde::Serialize;
use std::{sync::Arc, time::Duration};

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    #[default]
    Unknown,
    Healthy,
    Unhealthy,
    Unsupported,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthState {
    pub status: HealthStatus,
    pub checked_at: Option<String>,
    pub consecutive_failures: u32,
    pub error_message: Option<String>,
}

impl HealthState {
    fn record(&mut self, result: Result<(), RequestError>, manual: bool) {
        self.checked_at = Some(chrono::Utc::now().to_rfc3339());
        match result {
            Ok(()) => {
                self.status = HealthStatus::Healthy;
                self.consecutive_failures = 0;
                self.error_message = None;
            }
            Err(error) if error.code == Some(-32601) => {
                self.status = HealthStatus::Unsupported;
                self.consecutive_failures = 0;
                self.error_message = None;
            }
            Err(error) => {
                self.consecutive_failures = self.consecutive_failures.saturating_add(1);
                self.error_message = Some(redact_sensitive_stderr_values(&error.message));
                if manual || self.consecutive_failures >= 3 {
                    self.status = HealthStatus::Unhealthy;
                }
            }
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerHealthSnapshot {
    pub server_id: String,
    #[serde(flatten)]
    pub health: HealthState,
}

#[derive(Debug)]
pub enum HealthCheckError {
    NotFound,
    NotRunning,
    Busy,
    Changed,
}

impl ServerManager {
    pub async fn health_snapshots(&self) -> Vec<ServerHealthSnapshot> {
        self.slots
            .lock()
            .await
            .iter()
            .map(|(id, slot)| ServerHealthSnapshot {
                server_id: id.clone(),
                health: if matches!(slot.status, ServerStatus::Running) {
                    slot.health.clone()
                } else {
                    HealthState::default()
                },
            })
            .collect()
    }

    pub async fn check_health(
        &self,
        id: &str,
        manual: bool,
    ) -> Result<ServerHealthSnapshot, HealthCheckError> {
        let (session, token) = {
            let slots = self.slots.lock().await;
            let slot = slots.get(id).ok_or(HealthCheckError::NotFound)?;
            if !matches!(slot.status, ServerStatus::Running) {
                return Err(HealthCheckError::NotRunning);
            }
            (
                slot.session.clone().ok_or(HealthCheckError::NotRunning)?,
                slot.start_token,
            )
        };
        let client = session.try_lock().map_err(|_| HealthCheckError::Busy)?;
        let result = client.ping(Duration::from_secs(5)).await;
        drop(client);
        let snapshot = {
            let mut slots = self.slots.lock().await;
            let slot = slots.get_mut(id).ok_or(HealthCheckError::Changed)?;
            // 重启或停止后的旧结果不能污染新会话。
            if slot.start_token != token || !matches!(slot.status, ServerStatus::Running) {
                return Err(HealthCheckError::Changed);
            }
            let previous = slot.health.status;
            slot.health.record(result, manual);
            if slot.health.status == HealthStatus::Unhealthy && previous != HealthStatus::Unhealthy
            {
                tracing::warn!(server_id = id, error = ?slot.health.error_message, "MCP health check failed");
            }
            ServerHealthSnapshot {
                server_id: id.to_string(),
                health: slot.health.clone(),
            }
        };
        self.event_bus.emit(Evt::ServerHealth {
            snapshot: snapshot.clone(),
        });
        Ok(snapshot)
    }

    pub fn spawn_health_checker(self: &Arc<Self>) {
        let manager = self.clone();
        let mut events = self.event_bus.subscribe();
        tokio::spawn(async move {
            loop {
                let config = settings::get_settings(&manager.db)
                    .unwrap_or_else(|_| settings::default_settings());
                let deadline = tokio::time::Instant::now()
                    + Duration::from_secs(config.advanced.mcp_health_check_interval_seconds as u64);
                if config.advanced.mcp_health_checks_enabled {
                    let ids: Vec<_> = manager
                        .slots
                        .lock()
                        .await
                        .iter()
                        .filter(|(_, slot)| {
                            matches!(slot.status, ServerStatus::Running)
                                && slot.health.status != HealthStatus::Unsupported
                        })
                        .map(|(id, _)| id.clone())
                        .collect();
                    stream::iter(ids)
                        .for_each_concurrent(4, |id| {
                            let manager = manager.clone();
                            async move {
                                let _ = manager.check_health(&id, false).await;
                            }
                        })
                        .await;
                }
                loop {
                    tokio::select! {
                        _ = tokio::time::sleep_until(deadline), if config.advanced.mcp_health_checks_enabled => break,
                        event = events.recv() => match event {
                            Ok(Evt::SettingsChanged { .. }) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => break,
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                            _ => {},
                        },
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transient_failures_do_not_claim_the_process_stopped() {
        let mut state = HealthState::default();
        state.record(Ok(()), false);
        for _ in 0..2 {
            state.record(Err("timeout".into()), false);
        }
        assert_eq!(state.status, HealthStatus::Healthy);
        state.record(Err("timeout".into()), false);
        assert_eq!(state.status, HealthStatus::Unhealthy);
        state.record(Ok(()), false);
        assert_eq!(state.consecutive_failures, 0);
        assert_eq!(state.status, HealthStatus::Healthy);
    }

    #[test]
    fn manual_failure_is_visible_and_unsupported_ping_is_not_a_failure() {
        let mut state = HealthState::default();
        state.record(Err("timeout".into()), true);
        assert_eq!(state.status, HealthStatus::Unhealthy);
        state.record(
            Err(RequestError {
                code: Some(-32601),
                message: "Method not found".into(),
            }),
            false,
        );
        assert_eq!(state.status, HealthStatus::Unsupported);
        assert_eq!(state.consecutive_failures, 0);
    }
}
