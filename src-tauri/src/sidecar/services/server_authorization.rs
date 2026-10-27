use crate::sidecar::db::server_repo::ServerRepository;
use crate::sidecar::db::Database;
use crate::sidecar::mcp::transport::http_client::HttpCredentialProvider;
use crate::sidecar::mcp::transport::request_error::RequestError;
use crate::sidecar::services::event_bus::{EventBus, Evt};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{Duration as ChronoDuration, Utc};
use futures::future::BoxFuture;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
#[cfg(test)]
use std::sync::Mutex as StdMutex;
use tokio::sync::{oneshot, Mutex};

const CREDENTIAL_SERVICE: &str = "com.snowautumn.moor";
const ATTEMPT_TTL_SECONDS: i64 = 600;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorizationSnapshot {
    pub server_id: String,
    pub state: String,
    pub can_revoke: bool,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub issuer: Option<String>,
    pub client_id: Option<String>,
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevocationResult {
    pub snapshot: AuthorizationSnapshot,
    pub warning: Option<String>,
}

#[derive(Debug, Clone)]
struct AuthorizationRecord {
    status: String,
    issuer: Option<String>,
    client_id: Option<String>,
    client_secret_ref: Option<String>,
    resource: Option<String>,
    token_endpoint: Option<String>,
    revocation_endpoint: Option<String>,
    scopes: Vec<String>,
    access_token_ref: Option<String>,
    refresh_token_ref: Option<String>,
    expires_at: Option<String>,
    error_code: Option<String>,
    error_message: Option<String>,
}

impl AuthorizationRecord {
    fn required() -> Self {
        Self {
            status: "required".into(),
            issuer: None,
            client_id: None,
            client_secret_ref: None,
            resource: None,
            token_endpoint: None,
            revocation_endpoint: None,
            scopes: Vec::new(),
            access_token_ref: None,
            refresh_token_ref: None,
            expires_at: None,
            error_code: None,
            error_message: None,
        }
    }

    fn set_error(&mut self, message: Option<String>) {
        self.error_code = message
            .as_deref()
            .map(|message| authorization_error_code(message).to_string());
        self.error_message = message;
    }
}

pub trait CredentialStore: Send + Sync {
    fn get(&self, reference: &str) -> Result<Option<String>, String>;
    fn set(&self, reference: &str, value: &str) -> Result<(), String>;
    fn delete(&self, reference: &str) -> Result<(), String>;
}

struct KeyringCredentialStore;

impl KeyringCredentialStore {
    fn entry(reference: &str) -> Result<keyring::Entry, String> {
        keyring::Entry::new(CREDENTIAL_SERVICE, reference)
            .map_err(|_| "System credential store is unavailable".to_string())
    }
}

impl CredentialStore for KeyringCredentialStore {
    fn get(&self, reference: &str) -> Result<Option<String>, String> {
        match Self::entry(reference)?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err("System credential store is unavailable".into()),
        }
    }

    fn set(&self, reference: &str, value: &str) -> Result<(), String> {
        Self::entry(reference)?
            .set_password(value)
            .map_err(|_| "System credential store is unavailable".to_string())
    }

    fn delete(&self, reference: &str) -> Result<(), String> {
        match Self::entry(reference)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err("System credential store is unavailable".into()),
        }
    }
}

#[cfg(test)]
#[derive(Default)]
pub struct MemoryCredentialStore {
    values: StdMutex<HashMap<String, String>>,
    unavailable: bool,
    fail_delete: StdMutex<Option<String>>,
    fail_set: StdMutex<Option<String>>,
}

#[cfg(test)]
impl MemoryCredentialStore {
    pub fn unavailable() -> Self {
        Self {
            values: StdMutex::new(HashMap::new()),
            unavailable: true,
            fail_delete: StdMutex::new(None),
            fail_set: StdMutex::new(None),
        }
    }

    pub fn fail_delete_for_test(&self, reference: Option<String>) {
        *self.fail_delete.lock().unwrap() = reference;
    }

    pub fn fail_set_for_test(&self, reference: Option<String>) {
        *self.fail_set.lock().unwrap() = reference;
    }

    pub fn values_for_test(&self) -> Vec<(String, String)> {
        self.values
            .lock()
            .unwrap()
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect()
    }
}

#[cfg(test)]
impl CredentialStore for MemoryCredentialStore {
    fn get(&self, reference: &str) -> Result<Option<String>, String> {
        if self.unavailable {
            return Err("System credential store is unavailable".into());
        }
        Ok(self.values.lock().unwrap().get(reference).cloned())
    }

    fn set(&self, reference: &str, value: &str) -> Result<(), String> {
        if self.unavailable || self.fail_set.lock().unwrap().as_deref() == Some(reference) {
            return Err("System credential store is unavailable".into());
        }
        self.values
            .lock()
            .unwrap()
            .insert(reference.to_string(), value.to_string());
        Ok(())
    }

    fn delete(&self, reference: &str) -> Result<(), String> {
        if self.fail_delete.lock().unwrap().as_deref() == Some(reference) {
            return Err("System credential store is unavailable".into());
        }
        self.values.lock().unwrap().remove(reference);
        Ok(())
    }
}

struct OAuthCredentialProvider {
    store: Arc<dyn CredentialStore>,
    authorization: Arc<ServerAuthorization>,
    server_id: String,
    resource: String,
    server_url: String,
}

impl HttpCredentialProvider for OAuthCredentialProvider {
    fn headers(&self) -> BoxFuture<'static, Result<HashMap<String, String>, RequestError>> {
        let store = self.store.clone();
        let authorization = self.authorization.clone();
        let server_id = self.server_id.clone();
        let resource = self.resource.clone();
        let server_url = self.server_url.clone();
        Box::pin(async move {
            authorization
                .validate_provider(&server_id, &resource, &server_url)
                .await?;
            authorization.refresh_if_needed(&server_id).await?;
            let lifecycle = authorization.lifecycle(&server_id).await;
            let guard = lifecycle.lock().await;
            if guard.disabled || guard.deleting {
                return Err("OAuth authorization required".into());
            }
            authorization
                .ensure_server_url(&server_id, &server_url)
                .map_err(RequestError::from)?;
            let record = authorization
                .load(&server_id)
                .map_err(RequestError::from)?
                .filter(|record| {
                    record.status == "authorized" && record.resource.as_deref() == Some(&resource)
                })
                .ok_or("OAuth authorization required")?;
            let reference = record
                .access_token_ref
                .ok_or("OAuth authorization required")?;
            let token = store
                .get(&reference)
                .map_err(RequestError::from)?
                .ok_or("OAuth authorization required")?;
            Ok(HashMap::from([(
                "authorization".to_string(),
                format!("Bearer {token}"),
            )]))
        })
    }

    fn on_unauthorized(&self) -> BoxFuture<'static, Result<bool, RequestError>> {
        let authorization = self.authorization.clone();
        let server_id = self.server_id.clone();
        let resource = self.resource.clone();
        let server_url = self.server_url.clone();
        Box::pin(async move {
            authorization
                .validate_provider(&server_id, &resource, &server_url)
                .await?;
            authorization.refresh(&server_id, true).await?;
            Ok(true)
        })
    }

    fn on_insufficient_scope(
        &self,
        challenge: Option<String>,
    ) -> BoxFuture<'static, Result<(), RequestError>> {
        let authorization = self.authorization.clone();
        let server_id = self.server_id.clone();
        let resource = self.resource.clone();
        let server_url = self.server_url.clone();
        Box::pin(async move {
            authorization
                .require_step_up(&server_id, &resource, &server_url, challenge)
                .await
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct ResourceMetadata {
    authorization_servers: Vec<String>,
    scopes_supported: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct AuthorizationServerMetadata {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    registration_endpoint: Option<String>,
    revocation_endpoint: Option<String>,
    code_challenge_methods_supported: Option<Vec<String>>,
    #[serde(default)]
    authorization_response_iss_parameter_supported: bool,
}

#[derive(Debug, Deserialize)]
struct DynamicClientRegistration {
    client_id: String,
    client_secret: Option<String>,
}

struct Discovery {
    metadata: AuthorizationServerMetadata,
    resource: String,
    scopes: Vec<String>,
}

struct PendingAttempt {
    server_id: String,
    redirect_uri: String,
    code_verifier: String,
    client_id: String,
    client_secret: Option<String>,
    token_endpoint: String,
    revocation_endpoint: Option<String>,
    expected_issuer: String,
    issuer_parameter_required: bool,
    resource: String,
    scopes: Vec<String>,
    expires_at: chrono::DateTime<Utc>,
    generation: u64,
    shutdown: oneshot::Sender<()>,
}

#[derive(Default)]
pub(crate) struct AuthorizationLifecycle {
    pub(crate) generation: u64,
    pub(crate) deleting: bool,
    disabled: bool,
}

pub struct ServerAuthorization {
    db: Arc<Database>,
    event_bus: Arc<EventBus>,
    store: Arc<dyn CredentialStore>,
    client: reqwest::Client,
    attempts: Arc<Mutex<HashMap<String, PendingAttempt>>>,
    discoveries: Arc<Mutex<HashMap<String, u64>>>,
    lifecycles: Arc<Mutex<HashMap<String, Arc<Mutex<AuthorizationLifecycle>>>>>,
    refresh_locks: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
    refresh_completed: Arc<Mutex<HashMap<String, chrono::DateTime<Utc>>>>,
}

impl ServerAuthorization {
    pub fn new(db: Arc<Database>, event_bus: Arc<EventBus>) -> Self {
        Self::with_credential_store(db, event_bus, Arc::new(KeyringCredentialStore))
    }

    pub fn with_credential_store(
        db: Arc<Database>,
        event_bus: Arc<EventBus>,
        store: Arc<dyn CredentialStore>,
    ) -> Self {
        Self {
            db,
            event_bus,
            store,
            client: reqwest::Client::new(),
            attempts: Arc::new(Mutex::new(HashMap::new())),
            discoveries: Arc::new(Mutex::new(HashMap::new())),
            lifecycles: Arc::new(Mutex::new(HashMap::new())),
            refresh_locks: Arc::new(Mutex::new(HashMap::new())),
            refresh_completed: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn revoke(&self, server_id: &str) -> Result<RevocationResult, String> {
        let lifecycle = self.lifecycle(server_id).await;
        let mut guard = lifecycle.lock().await;
        if guard.deleting {
            return Err("Server is being deleted".into());
        }
        let record = self.load(server_id)?;
        let mut warning = None;
        let mut requests = Vec::new();
        if let Some(record) = record {
            if let Some(endpoint) = record.revocation_endpoint {
                let endpoint = match Url::parse(&endpoint) {
                    Ok(endpoint) => match validate_oauth_http_url(&endpoint) {
                        Ok(()) => Some(endpoint),
                        Err(_) => None,
                    },
                    Err(_) => None,
                };
                let Some(endpoint) = endpoint else {
                    warning = Some(
                        "Provider revocation failed; local credentials were removed.".to_string(),
                    );
                    return self
                        .cleanup_locked(server_id, &mut guard)
                        .await
                        .map(|snapshot| RevocationResult { snapshot, warning });
                };
                let client_secret = match &record.client_secret_ref {
                    Some(reference) => match self.store.get(reference) {
                        Ok(secret) => secret,
                        Err(_) => {
                            warning = Some(
                                "Provider revocation failed; local credentials were removed."
                                    .to_string(),
                            );
                            None
                        }
                    },
                    None => None,
                };
                for (token, hint) in [
                    (record.access_token_ref, "access_token"),
                    (record.refresh_token_ref, "refresh_token"),
                ] {
                    let Some(reference) = token else {
                        continue;
                    };
                    let token = match self.store.get(&reference) {
                        Ok(Some(token)) => token,
                        Ok(None) => continue,
                        Err(_) => {
                            warning = Some(
                                "Provider revocation failed; local credentials were removed."
                                    .to_string(),
                            );
                            continue;
                        }
                    };
                    let mut form = HashMap::from([
                        ("token".to_string(), token),
                        ("token_type_hint".to_string(), hint.to_string()),
                    ]);
                    if let Some(client_id) = &record.client_id {
                        form.insert("client_id".to_string(), client_id.clone());
                    }
                    if let Some(secret) = &client_secret {
                        form.insert("client_secret".to_string(), secret.clone());
                    }
                    requests.push((endpoint.clone(), form));
                }
            }
        }
        let snapshot = self.cleanup_locked(server_id, &mut guard).await?;
        drop(guard);
        let remote = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            let mut failed = false;
            for (endpoint, form) in requests {
                let response = self.client.post(endpoint).form(&form).send().await;
                if !matches!(response, Ok(ref response) if response.status().is_success()) {
                    failed = true;
                }
            }
            failed
        })
        .await;
        if !matches!(remote, Ok(false)) {
            warning =
                Some("Provider revocation failed; local credentials were removed.".to_string());
        }
        Ok(RevocationResult { snapshot, warning })
    }

    pub(crate) async fn lifecycle(&self, server_id: &str) -> Arc<Mutex<AuthorizationLifecycle>> {
        self.lifecycles
            .lock()
            .await
            .entry(server_id.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(AuthorizationLifecycle::default())))
            .clone()
    }

    pub(crate) async fn cleanup_locked(
        &self,
        server_id: &str,
        lifecycle: &mut AuthorizationLifecycle,
    ) -> Result<AuthorizationSnapshot, String> {
        lifecycle.generation = lifecycle.generation.wrapping_add(1);
        lifecycle.disabled = true;
        let stale = {
            let mut attempts = self.attempts.lock().await;
            let states: Vec<String> = attempts
                .iter()
                .filter(|(_, attempt)| attempt.server_id == server_id)
                .map(|(state, _)| state.clone())
                .collect();
            states
                .into_iter()
                .filter_map(|state| attempts.remove(&state))
                .collect::<Vec<_>>()
        };
        for attempt in stale {
            let _ = attempt.shutdown.send(());
        }
        self.discoveries.lock().await.remove(server_id);
        self.refresh_completed.lock().await.remove(server_id);
        let Some(mut record) = self.load(server_id)? else {
            return self.emit_snapshot(server_id).await;
        };
        record.status = "required".into();
        record.set_error(None);
        self.save(server_id, &record)?;
        let mut failed = false;
        for reference in [
            &mut record.access_token_ref,
            &mut record.refresh_token_ref,
            &mut record.client_secret_ref,
        ] {
            if let Some(value) = reference.as_deref() {
                if let Err(error) = self.store.delete(value) {
                    tracing::warn!(server_id, %error, "OAuth credential cleanup failed");
                    failed = true;
                } else {
                    *reference = None;
                }
            }
        }
        if failed {
            record.set_error(Some("System credential store is unavailable".into()));
            self.save(server_id, &record)?;
            self.emit_snapshot(server_id).await?;
            return Err("System credential store is unavailable".into());
        }
        self.db.run(
            "DELETE FROM server_authorizations WHERE server_id = ?1",
            &[&server_id],
        )?;
        self.save(server_id, &AuthorizationRecord::required())?;
        self.emit_snapshot(server_id).await
    }

    async fn require_step_up(
        &self,
        server_id: &str,
        resource: &str,
        server_url: &str,
        challenge: Option<String>,
    ) -> Result<(), RequestError> {
        let required = challenge
            .as_deref()
            .and_then(|challenge| challenge_parameter(challenge, "scope"))
            .map(|scope| {
                scope
                    .split_whitespace()
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let is_insufficient_scope = challenge
            .as_deref()
            .and_then(|challenge| challenge_parameter(challenge, "error"))
            .is_some_and(|error| error == "insufficient_scope");
        if required.is_empty() && !is_insufficient_scope {
            return Ok(());
        }
        let lifecycle = self.lifecycle(server_id).await;
        let mut guard = lifecycle.lock().await;
        if guard.disabled || guard.deleting {
            return Err("OAuth authorization required".into());
        }
        self.ensure_server_url(server_id, server_url)
            .map_err(RequestError::from)?;
        let mut record = self
            .load(server_id)
            .map_err(RequestError::from)?
            .ok_or("OAuth authorization required")?;
        if record.status != "authorized" || record.resource.as_deref() != Some(resource) {
            return Err("OAuth authorization required".into());
        }
        guard.generation = guard.generation.wrapping_add(1);
        for scope in required {
            if !record.scopes.contains(&scope) {
                record.scopes.push(scope);
            }
        }
        record.status = "required".into();
        record.set_error(Some("Additional OAuth scopes are required".into()));
        self.save(server_id, &record).map_err(RequestError::from)?;
        self.emit_snapshot(server_id)
            .await
            .map_err(RequestError::from)?;
        Ok(())
    }

    async fn refresh_if_needed(&self, server_id: &str) -> Result<(), RequestError> {
        let record = self
            .load(server_id)
            .map_err(RequestError::from)?
            .ok_or("OAuth authorization required")?;
        let Some(expires_at) = record.expires_at.as_deref() else {
            return Ok(());
        };
        let expires_at = chrono::DateTime::parse_from_rfc3339(expires_at)
            .map_err(|_| RequestError::from("OAuth expiry is invalid"))?;
        if expires_at > Utc::now() + ChronoDuration::seconds(60) {
            return Ok(());
        }
        self.refresh(server_id, false).await
    }

    async fn require_refresh(
        &self,
        server_id: &str,
        generation: u64,
        message: &str,
    ) -> RequestError {
        let lifecycle = self.lifecycle(server_id).await;
        let guard = lifecycle.lock().await;
        if !guard.deleting && guard.generation == generation {
            if let Ok(Some(mut record)) = self.load(server_id) {
                record.status = "required".into();
                record.set_error(Some(message.to_string()));
                let _ = self.save(server_id, &record);
                let _ = self.emit_snapshot(server_id).await;
            }
        }
        RequestError::from(message)
    }

    pub async fn refresh(&self, server_id: &str, force: bool) -> Result<(), RequestError> {
        let lock = {
            let mut locks = self.refresh_locks.lock().await;
            locks
                .entry(server_id.to_string())
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone()
        };
        let _guard = lock.lock().await;
        let lifecycle = self.lifecycle(server_id).await;
        let guard = lifecycle.lock().await;
        let generation = guard.generation;
        if guard.disabled {
            return Err("OAuth authorization required".into());
        }
        let record = self
            .load(server_id)
            .map_err(RequestError::from)?
            .filter(|record| record.status == "authorized")
            .ok_or("OAuth authorization required")?;
        let resource = record
            .resource
            .as_deref()
            .ok_or("OAuth authorization required")?;
        self.ensure_current(server_id, resource, &guard, generation)
            .map_err(RequestError::from)?;
        if self
            .refresh_completed
            .lock()
            .await
            .get(server_id)
            .is_some_and(|completed| *completed > Utc::now() - ChronoDuration::seconds(1))
        {
            return Ok(());
        }
        if !force {
            let Some(expires_at) = record.expires_at.as_deref() else {
                return Ok(());
            };
            let expires_at = chrono::DateTime::parse_from_rfc3339(expires_at)
                .map_err(|_| RequestError::from("OAuth expiry is invalid"))?;
            if expires_at > Utc::now() + ChronoDuration::seconds(60) {
                return Ok(());
            }
        }
        let refresh_ref = record.refresh_token_ref.clone();
        let refresh_token = refresh_ref
            .as_ref()
            .and_then(|reference| self.store.get(reference).ok().flatten());
        let client_secret = match &record.client_secret_ref {
            Some(reference) => self.store.get(reference).map_err(RequestError::from)?,
            None => None,
        };
        drop(guard);
        let Some(refresh_ref) = refresh_ref else {
            return Err(self
                .require_refresh(server_id, generation, "OAuth refresh token is unavailable")
                .await);
        };
        let refresh_token = match refresh_token {
            Some(token) => token,
            None => {
                return Err(self
                    .require_refresh(server_id, generation, "OAuth refresh token is unavailable")
                    .await);
            }
        };
        let Some(endpoint) = record.token_endpoint.clone() else {
            return Err(self
                .require_refresh(
                    server_id,
                    generation,
                    "OAuth token endpoint is unavailable; reauthorize the Server",
                )
                .await);
        };
        let Some(client_id) = record.client_id.clone() else {
            return Err(self
                .require_refresh(
                    server_id,
                    generation,
                    "OAuth client registration is unavailable",
                )
                .await);
        };
        let mut form = HashMap::from([
            ("grant_type".to_string(), "refresh_token".to_string()),
            ("refresh_token".to_string(), refresh_token),
            ("client_id".to_string(), client_id),
            (
                "resource".to_string(),
                record.resource.clone().unwrap_or_default(),
            ),
        ]);
        if let Some(secret) = client_secret {
            form.insert("client_secret".to_string(), secret);
        }
        let response = self
            .client
            .post(endpoint)
            .form(&form)
            .send()
            .await
            .map_err(|_| RequestError::from("OAuth token endpoint is unavailable"))?;
        if !response.status().is_success() {
            return Err(self
                .require_refresh(
                    server_id,
                    generation,
                    "OAuth refresh was rejected; reauthorize the Server",
                )
                .await);
        }
        let token = response
            .json::<TokenResponse>()
            .await
            .map_err(|_| RequestError::from("Invalid OAuth refresh response"))?;
        if token.access_token.is_empty() {
            return Err(RequestError::from("Invalid OAuth refresh response"));
        }
        let guard = lifecycle.lock().await;
        self.ensure_current(
            server_id,
            record.resource.as_deref().unwrap_or_default(),
            &guard,
            generation,
        )
        .map_err(RequestError::from)?;
        let access_ref = access_token_ref(server_id);
        self.store
            .set(&access_ref, &token.access_token)
            .map_err(RequestError::from)?;
        if let Some(refresh) = &token.refresh_token {
            self.store
                .set(&refresh_ref, refresh)
                .map_err(RequestError::from)?;
        }
        let mut updated = record;
        updated.status = "authorized".into();
        updated.access_token_ref = Some(access_ref);
        updated.refresh_token_ref = Some(refresh_ref);
        updated.expires_at = token
            .expires_in
            .map(|seconds| (Utc::now() + ChronoDuration::seconds(seconds)).to_rfc3339());
        updated.set_error(None);
        self.save(server_id, &updated).map_err(RequestError::from)?;
        self.refresh_completed
            .lock()
            .await
            .insert(server_id.to_string(), Utc::now());
        self.emit_snapshot(server_id)
            .await
            .map_err(RequestError::from)?;
        drop(guard);
        drop(_guard);
        Ok(())
    }

    async fn validate_provider(
        &self,
        server_id: &str,
        resource: &str,
        server_url: &str,
    ) -> Result<(), RequestError> {
        let lifecycle = self.lifecycle(server_id).await;
        let guard = lifecycle.lock().await;
        if guard.disabled || guard.deleting {
            return Err("OAuth authorization required".into());
        }
        self.ensure_server_url(server_id, server_url)
            .map_err(RequestError::from)?;
        let record = self
            .load(server_id)
            .map_err(RequestError::from)?
            .ok_or("OAuth authorization required")?;
        if record.status != "authorized" || record.resource.as_deref() != Some(resource) {
            return Err("OAuth authorization required".into());
        }
        Ok(())
    }

    pub async fn credential_provider(
        &self,
        server_id: &str,
        server_url: &str,
    ) -> Result<Option<Arc<dyn HttpCredentialProvider>>, String> {
        let lifecycle = self.lifecycle(server_id).await;
        let guard = lifecycle.lock().await;
        if guard.deleting {
            return Err("Server is being deleted".into());
        }
        let Some(record) = self.load(server_id)? else {
            return Ok(None);
        };
        if record.status != "authorized" {
            return Ok(None);
        }
        if guard.disabled {
            return Err("OAuth authorization requires a new grant".into());
        }
        let resource =
            canonical_resource(&Url::parse(server_url).map_err(|_| "Invalid MCP Server URL")?)?;
        self.ensure_server_url(server_id, server_url)?;
        if record.resource.as_deref() != Some(&resource) || record.access_token_ref.is_none() {
            return Err("OAuth authorization requires a new grant".into());
        }
        Ok(Some(Arc::new(OAuthCredentialProvider {
            store: self.store.clone(),
            authorization: self.shared(),
            server_id: server_id.to_string(),
            resource,
            server_url: server_url.to_string(),
        })))
    }

    pub async fn snapshot(&self, server_id: &str) -> Result<AuthorizationSnapshot, String> {
        let lifecycle = self.lifecycle(server_id).await;
        let _guard = lifecycle.lock().await;
        self.snapshot_locked(server_id).await
    }

    async fn snapshot_locked(&self, server_id: &str) -> Result<AuthorizationSnapshot, String> {
        let expired = {
            let mut attempts = self.attempts.lock().await;
            let expired: Vec<String> = attempts
                .iter()
                .filter(|(_, attempt)| {
                    attempt.server_id == server_id && attempt.expires_at <= Utc::now()
                })
                .map(|(state, _)| state.clone())
                .collect();
            expired
                .into_iter()
                .filter_map(|state| attempts.remove(&state))
                .collect::<Vec<_>>()
        };
        let has_expired = !expired.is_empty();
        for attempt in expired {
            let _ = attempt.shutdown.send(());
        }
        if has_expired {
            let mut record = self
                .load(server_id)?
                .unwrap_or_else(AuthorizationRecord::required);
            if record.status != "authorized" {
                record.status = "error".into();
            }
            record.set_error(Some("OAuth attempt expired".into()));
            self.save(server_id, &record)?;
        }
        let attempts = self.attempts.lock().await;
        let record = self.load(server_id)?;
        let authorizing = attempts
            .values()
            .any(|attempt| attempt.server_id == server_id)
            || self.discoveries.lock().await.contains_key(server_id);
        let snapshot = self.snapshot_with_attempt(server_id, record, authorizing);
        if has_expired {
            self.event_bus.emit(Evt::ServerAuthorization {
                snapshot: snapshot.clone(),
            });
        }
        Ok(snapshot)
    }

    fn snapshot_with_attempt(
        &self,
        server_id: &str,
        record: Option<AuthorizationRecord>,
        authorizing: bool,
    ) -> AuthorizationSnapshot {
        let can_revoke = authorizing
            || record.as_ref().is_some_and(|record| {
                record.client_id.is_some()
                    || record.access_token_ref.is_some()
                    || record.refresh_token_ref.is_some()
                    || record.client_secret_ref.is_some()
            });
        let state = if authorizing {
            "authorizing".to_string()
        } else if record
            .as_ref()
            .is_some_and(|record| record.status == "required")
        {
            "required".to_string()
        } else if record
            .as_ref()
            .is_some_and(|record| record.error_message.is_some())
        {
            "error".to_string()
        } else {
            record
                .as_ref()
                .map(|record| record.status.clone())
                .unwrap_or_else(|| "not_required".to_string())
        };
        let record = record.unwrap_or_else(AuthorizationRecord::required);
        AuthorizationSnapshot {
            server_id: server_id.to_string(),
            state,
            can_revoke,
            error_code: record.error_code,
            error_message: record.error_message,
            issuer: record.issuer,
            client_id: record.client_id,
            scopes: record.scopes,
        }
    }

    fn persist_client_secret(
        &self,
        record: &mut AuthorizationRecord,
        server_id: &str,
        secret: &str,
    ) -> Result<(), String> {
        record.client_secret_ref = Some(client_secret_ref(server_id));
        self.store.set(&client_secret_ref(server_id), secret)
    }

    pub async fn register_client(
        &self,
        server_id: &str,
        client_id: &str,
        client_secret: Option<&str>,
    ) -> Result<AuthorizationSnapshot, String> {
        if client_id.trim().is_empty() {
            return Err("Client ID is required".into());
        }
        let lifecycle = self.lifecycle(server_id).await;
        let mut guard = lifecycle.lock().await;
        if guard.deleting {
            return Err("Server is being deleted".into());
        }
        if let Some(previous) = self.load(server_id)? {
            let previous_secret = match &previous.client_secret_ref {
                Some(reference) => self.store.get(reference)?,
                None => None,
            };
            let client_changed = previous.client_id.as_deref() != Some(client_id.trim());
            let secret_changed = client_secret
                .filter(|secret| !secret.is_empty())
                .is_some_and(|secret| previous_secret.as_deref() != Some(secret));
            if client_changed || secret_changed {
                self.cleanup_locked(server_id, &mut guard).await?;
            } else {
                return self.emit_snapshot(server_id).await;
            }
        }
        let mut record = self
            .load(server_id)?
            .unwrap_or_else(AuthorizationRecord::required);
        record.status = "required".into();
        record.set_error(None);
        record.client_id = Some(client_id.trim().to_string());
        if let Some(secret) = client_secret.filter(|secret| !secret.is_empty()) {
            self.persist_client_secret(&mut record, server_id, secret)?;
        }
        self.save(server_id, &record)?;
        self.emit_snapshot(server_id).await
    }

    pub async fn start(
        &self,
        server_id: &str,
        requested_client_id: Option<&str>,
    ) -> Result<Value, String> {
        let lifecycle = self.lifecycle(server_id).await;
        let mut guard = lifecycle.lock().await;
        if guard.deleting {
            return Err("Server is being deleted".into());
        }
        guard.generation = guard.generation.wrapping_add(1);
        let mut generation = guard.generation;
        {
            let mut attempts = self.attempts.lock().await;
            let states: Vec<_> = attempts
                .iter()
                .filter(|(_, attempt)| attempt.server_id == server_id)
                .map(|(state, _)| state.clone())
                .collect();
            for state in states {
                if let Some(attempt) = attempts.remove(&state) {
                    let _ = attempt.shutdown.send(());
                }
            }
        }
        let record = self
            .load(server_id)?
            .unwrap_or_else(AuthorizationRecord::required);
        let dynamic_registration =
            requested_client_id.is_some_and(|client_id| client_id.trim().is_empty());
        let (client_id, client_secret) = if dynamic_registration {
            (None, None)
        } else {
            let client_secret = match &record.client_secret_ref {
                Some(reference) => self.store.get(reference)?,
                None => None,
            };
            (record.client_id.clone(), client_secret)
        };
        let server = ServerRepository::new(&self.db)
            .find_by_id(server_id)?
            .ok_or("Server not found")?;
        let server_url = server.url.ok_or("OAuth requires an HTTP Server URL")?;
        self.discoveries
            .lock()
            .await
            .insert(server_id.to_string(), generation);
        self.emit_snapshot(server_id).await?;
        drop(guard);
        match self
            .start_inner(
                server_id,
                &server_url,
                client_id,
                client_secret,
                record.scopes.clone(),
                record.issuer.clone(),
                record.resource.clone(),
                &mut generation,
            )
            .await
        {
            Ok(result) => Ok(result),
            Err(error) => {
                let guard = lifecycle.lock().await;
                if guard.deleting || guard.generation != generation {
                    return Err(error);
                }
                self.discoveries.lock().await.remove(server_id);
                let stale = {
                    let mut attempts = self.attempts.lock().await;
                    let stale = attempts.iter().find_map(|(state, attempt)| {
                        (attempt.server_id == server_id).then_some(state.clone())
                    });
                    stale.and_then(|state| attempts.remove(&state))
                };
                if let Some(stale) = stale {
                    let _ = stale.shutdown.send(());
                }
                self.set_status(server_id, "error", Some(error.clone()))
                    .await?;
                Err(error)
            }
        }
    }

    async fn start_inner(
        &self,
        server_id: &str,
        server_url: &str,
        client_id: Option<String>,
        client_secret: Option<String>,
        previous_scopes: Vec<String>,
        previous_issuer: Option<String>,
        previous_resource: Option<String>,
        generation: &mut u64,
    ) -> Result<Value, String> {
        let mut discovery = self.discover(server_url).await?;
        for scope in previous_scopes {
            if !discovery.scopes.contains(&scope) {
                discovery.scopes.push(scope);
            }
        }
        validate_oauth_http_url(
            &Url::parse(&discovery.metadata.authorization_endpoint)
                .map_err(|_| "Invalid authorization endpoint")?,
        )?;
        validate_oauth_http_url(
            &Url::parse(&discovery.metadata.token_endpoint)
                .map_err(|_| "Invalid token endpoint")?,
        )?;
        let context_changed = previous_issuer
            .as_ref()
            .is_some_and(|issuer| *issuer != discovery.metadata.issuer)
            || previous_resource
                .as_ref()
                .is_some_and(|resource| *resource != discovery.resource);
        let lifecycle = self.lifecycle(server_id).await;
        let mut guard = lifecycle.lock().await;
        self.ensure_current(server_id, &discovery.resource, &guard, *generation)?;
        let (client_id, client_secret) = if context_changed {
            self.cleanup_locked(server_id, &mut guard).await?;
            *generation = guard.generation;
            self.discoveries
                .lock()
                .await
                .insert(server_id.to_string(), *generation);
            (None, None)
        } else {
            (client_id, client_secret)
        };
        drop(guard);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|error| format!("Failed to bind OAuth callback: {error}"))?;
        let callback_addr = listener.local_addr().map_err(|error| error.to_string())?;
        let redirect_uri = format!("http://{callback_addr}/oauth/callback");
        let (client_id, client_secret) = self
            .resolve_client(
                server_id,
                &discovery.metadata,
                client_id,
                client_secret,
                &redirect_uri,
                &discovery.resource,
                generation,
            )
            .await?;
        let state = random_url_safe(32)?;
        let code_verifier = random_url_safe(48)?;
        let code_challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(code_verifier.as_bytes()));
        let expires_at = Utc::now() + ChronoDuration::seconds(ATTEMPT_TTL_SECONDS);
        let (shutdown, stopped) = oneshot::channel();
        {
            let guard = lifecycle.lock().await;
            self.ensure_current(server_id, &discovery.resource, &guard, *generation)?;
            let mut attempts = self.attempts.lock().await;
            let previous = attempts.iter().find_map(|(state, attempt)| {
                (attempt.server_id == server_id).then_some(state.clone())
            });
            if let Some(previous) = previous.and_then(|state| attempts.remove(&state)) {
                let _ = previous.shutdown.send(());
            }
            attempts.insert(
                state.clone(),
                PendingAttempt {
                    server_id: server_id.to_string(),
                    redirect_uri: redirect_uri.clone(),
                    code_verifier: code_verifier.clone(),
                    client_id: client_id.clone(),
                    client_secret,
                    token_endpoint: discovery.metadata.token_endpoint.clone(),
                    revocation_endpoint: discovery.metadata.revocation_endpoint.clone(),
                    expected_issuer: discovery.metadata.issuer.clone(),
                    issuer_parameter_required: discovery
                        .metadata
                        .authorization_response_iss_parameter_supported,
                    resource: discovery.resource.clone(),
                    scopes: discovery.scopes.clone(),
                    expires_at,
                    generation: *generation,
                    shutdown,
                },
            );
            drop(attempts);
            self.discoveries.lock().await.remove(server_id);
            self.emit_snapshot(server_id).await?;
        }
        let authorization = self.shared();
        tokio::spawn(async move {
            let app = Router::new()
                .route("/oauth/callback", get(oauth_callback))
                .with_state(authorization);
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = stopped.await;
                })
                .await;
        });
        let mut authorization_url = Url::parse(&discovery.metadata.authorization_endpoint)
            .map_err(|_| "Invalid authorization endpoint")?;
        authorization_url
            .query_pairs_mut()
            .append_pair("response_type", "code")
            .append_pair("client_id", &client_id)
            .append_pair("redirect_uri", &redirect_uri)
            .append_pair("state", &state)
            .append_pair("code_challenge", &code_challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("resource", &discovery.resource);
        if !discovery.scopes.is_empty() {
            authorization_url
                .query_pairs_mut()
                .append_pair("scope", &discovery.scopes.join(" "));
        }
        Ok(serde_json::json!({
            "authorizationUrl": authorization_url.to_string(),
            "expiresAt": expires_at.to_rfc3339(),
        }))
    }

    async fn resolve_client(
        &self,
        server_id: &str,
        metadata: &AuthorizationServerMetadata,
        client_id: Option<String>,
        client_secret: Option<String>,
        redirect_uri: &str,
        resource: &str,
        generation: &mut u64,
    ) -> Result<(String, Option<String>), String> {
        if let Some(client_id) = client_id {
            return Ok((client_id, client_secret));
        }
        let endpoint = metadata
            .registration_endpoint
            .as_deref()
            .ok_or("Register an OAuth client before authorizing")?;
        let endpoint = Url::parse(endpoint).map_err(|_| "Invalid client registration endpoint")?;
        validate_oauth_http_url(&endpoint)?;
        let response = self
            .client
            .post(endpoint)
            .json(&serde_json::json!({
                "client_name": "Moor",
                "redirect_uris": [redirect_uri],
                "grant_types": ["authorization_code", "refresh_token"],
                "response_types": ["code"],
                "token_endpoint_auth_method": "client_secret_post"
            }))
            .send()
            .await
            .map_err(|_| "Dynamic client registration failed")?;
        if !response.status().is_success() {
            return Err("Dynamic client registration was rejected".into());
        }
        let registration = response
            .json::<DynamicClientRegistration>()
            .await
            .map_err(|_| "Invalid dynamic client registration response")?;
        if registration.client_id.trim().is_empty() {
            return Err("Dynamic client registration returned no client ID".into());
        }
        let lifecycle = self.lifecycle(server_id).await;
        let mut guard = lifecycle.lock().await;
        self.ensure_current(server_id, resource, &guard, *generation)?;
        let mut record = self
            .load(server_id)?
            .unwrap_or_else(AuthorizationRecord::required);
        if record.client_id.is_some() || record.client_secret_ref.is_some() {
            self.cleanup_locked(server_id, &mut guard).await?;
            *generation = guard.generation;
            self.discoveries
                .lock()
                .await
                .insert(server_id.to_string(), *generation);
            record = self
                .load(server_id)?
                .unwrap_or_else(AuthorizationRecord::required);
        }
        record.client_id = Some(registration.client_id.clone());
        if let Some(secret) = registration.client_secret.as_deref() {
            self.persist_client_secret(&mut record, server_id, secret)?;
        }
        self.save(server_id, &record)?;
        Ok((registration.client_id, registration.client_secret))
    }

    async fn discover(&self, server_url: &str) -> Result<Discovery, String> {
        let url = Url::parse(server_url).map_err(|_| "Invalid MCP Server URL")?;
        let resource = canonical_resource(&url)?;
        let response = self
            .client
            .post(url.clone())
            .header("accept", "application/json, text/event-stream")
            .json(&serde_json::json!({
                "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {}
            }))
            .send()
            .await
            .map_err(|error| format!("MCP authorization discovery failed: {error}"))?;
        let challenge = response
            .headers()
            .get("www-authenticate")
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let metadata_url = challenge
            .as_deref()
            .and_then(|value| challenge_parameter(value, "resource_metadata"));
        let scope = challenge
            .as_deref()
            .and_then(|value| challenge_parameter(value, "scope"));
        let resource_metadata = self.fetch_resource_metadata(&url, metadata_url).await?;
        let issuer = resource_metadata
            .authorization_servers
            .first()
            .ok_or("Protected resource metadata has no authorization servers")?;
        let metadata = self.fetch_authorization_metadata(issuer).await?;
        if !metadata
            .code_challenge_methods_supported
            .as_ref()
            .is_some_and(|methods| methods.iter().any(|method| method == "S256"))
        {
            return Err("Authorization server does not support PKCE S256".into());
        }
        Ok(Discovery {
            metadata,
            resource,
            scopes: scope
                .map(|value| value.split_whitespace().map(str::to_string).collect())
                .or(resource_metadata.scopes_supported)
                .unwrap_or_default(),
        })
    }

    async fn fetch_resource_metadata(
        &self,
        server_url: &Url,
        metadata_url: Option<String>,
    ) -> Result<ResourceMetadata, String> {
        let candidates = match metadata_url {
            Some(url) => vec![Url::parse(&url).map_err(|_| "Invalid resource metadata URL")?],
            None => protected_resource_candidates(server_url)?,
        };
        for candidate in candidates {
            let response = self.client.get(candidate).send().await;
            if let Ok(response) = response {
                if response.status().is_success() {
                    return response
                        .json::<ResourceMetadata>()
                        .await
                        .map_err(|_| "Invalid protected resource metadata".into());
                }
            }
        }
        Err("Protected resource metadata is unavailable".into())
    }

    async fn fetch_authorization_metadata(
        &self,
        issuer: &str,
    ) -> Result<AuthorizationServerMetadata, String> {
        let issuer_url = Url::parse(issuer).map_err(|_| "Invalid authorization server issuer")?;
        validate_oauth_http_url(&issuer_url)?;
        for candidate in authorization_metadata_candidates(&issuer_url)? {
            let response = self.client.get(candidate).send().await;
            if let Ok(response) = response {
                if response.status().is_success() {
                    let metadata = response
                        .json::<AuthorizationServerMetadata>()
                        .await
                        .map_err(|_| "Invalid authorization server metadata")?;
                    if metadata.issuer != issuer {
                        return Err("Authorization server issuer mismatch".into());
                    }
                    return Ok(metadata);
                }
            }
        }
        Err("Authorization server metadata is unavailable".into())
    }

    fn shared(&self) -> Arc<Self> {
        Arc::new(Self {
            db: self.db.clone(),
            event_bus: self.event_bus.clone(),
            store: self.store.clone(),
            client: self.client.clone(),
            attempts: self.attempts.clone(),
            discoveries: self.discoveries.clone(),
            lifecycles: self.lifecycles.clone(),
            refresh_locks: self.refresh_locks.clone(),
            refresh_completed: self.refresh_completed.clone(),
        })
    }

    fn ensure_current(
        &self,
        server_id: &str,
        resource: &str,
        lifecycle: &AuthorizationLifecycle,
        generation: u64,
    ) -> Result<(), String> {
        if lifecycle.deleting || lifecycle.generation != generation {
            return Err("OAuth authorization was superseded".into());
        }
        let server = ServerRepository::new(&self.db)
            .find_by_id(server_id)?
            .ok_or("Server not found")?;
        let current = server
            .url
            .as_deref()
            .and_then(|url| Url::parse(url).ok())
            .and_then(|url| canonical_resource(&url).ok());
        if current.as_deref() != Some(resource) {
            return Err("OAuth resource changed".into());
        }
        Ok(())
    }

    fn ensure_server_url(&self, server_id: &str, server_url: &str) -> Result<(), String> {
        let server = ServerRepository::new(&self.db)
            .find_by_id(server_id)?
            .ok_or("Server not found")?;
        if server.url.as_deref() != Some(server_url) {
            return Err("OAuth resource changed".into());
        }
        Ok(())
    }

    async fn set_attempt_error(&self, attempt: &PendingAttempt, message: String) {
        let lifecycle = self.lifecycle(&attempt.server_id).await;
        let guard = lifecycle.lock().await;
        if self
            .ensure_current(
                &attempt.server_id,
                &attempt.resource,
                &guard,
                attempt.generation,
            )
            .is_ok()
        {
            let _ = self
                .set_status(&attempt.server_id, "error", Some(message))
                .await;
        }
    }

    async fn set_status(
        &self,
        server_id: &str,
        status: &str,
        error_message: Option<String>,
    ) -> Result<(), String> {
        let mut record = self
            .load(server_id)?
            .unwrap_or_else(AuthorizationRecord::required);
        if status != "error" || record.status != "authorized" {
            record.status = status.to_string();
        }
        record.set_error(error_message);
        self.save(server_id, &record)?;
        self.emit_snapshot(server_id).await.map(|_| ())
    }

    async fn emit_snapshot(&self, server_id: &str) -> Result<AuthorizationSnapshot, String> {
        let snapshot = self.snapshot_locked(server_id).await?;
        self.event_bus.emit(Evt::ServerAuthorization {
            snapshot: snapshot.clone(),
        });
        Ok(snapshot)
    }

    fn load(&self, server_id: &str) -> Result<Option<AuthorizationRecord>, String> {
        self.db.query_one(
            "SELECT status, issuer, client_id, client_secret_ref, resource, token_endpoint, revocation_endpoint, scopes, access_token_ref, refresh_token_ref, expires_at, error_code, error_message FROM server_authorizations WHERE server_id = ?1",
            &[&server_id],
            |row| {
                let scopes: String = row.get(7)?;
                Ok(AuthorizationRecord {
                    status: row.get(0)?,
                    issuer: row.get(1)?,
                    client_id: row.get(2)?,
                    client_secret_ref: row.get(3)?,
                    resource: row.get(4)?,
                    token_endpoint: row.get(5)?,
                    revocation_endpoint: row.get(6)?,
                    scopes: serde_json::from_str(&scopes).unwrap_or_default(),
                    access_token_ref: row.get(8)?,
                    refresh_token_ref: row.get(9)?,
                    expires_at: row.get(10)?,
                    error_code: row.get(11)?,
                    error_message: row.get(12)?,
                })
            },
        )
    }

    fn save(&self, server_id: &str, record: &AuthorizationRecord) -> Result<(), String> {
        let scopes = serde_json::to_string(&record.scopes).map_err(|error| error.to_string())?;
        let now = Utc::now().to_rfc3339();
        self.db.run(
            "INSERT INTO server_authorizations (server_id, status, issuer, client_id, client_secret_ref, resource, token_endpoint, revocation_endpoint, scopes, access_token_ref, refresh_token_ref, expires_at, error_code, error_message, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15) ON CONFLICT(server_id) DO UPDATE SET status = excluded.status, issuer = excluded.issuer, client_id = excluded.client_id, client_secret_ref = excluded.client_secret_ref, resource = excluded.resource, token_endpoint = excluded.token_endpoint, revocation_endpoint = excluded.revocation_endpoint, scopes = excluded.scopes, access_token_ref = excluded.access_token_ref, refresh_token_ref = excluded.refresh_token_ref, expires_at = excluded.expires_at, error_code = excluded.error_code, error_message = excluded.error_message, updated_at = excluded.updated_at",
            &[
                &server_id,
                &record.status,
                &record.issuer,
                &record.client_id,
                &record.client_secret_ref,
                &record.resource,
                &record.token_endpoint,
                &record.revocation_endpoint,
                &scopes,
                &record.access_token_ref,
                &record.refresh_token_ref,
                &record.expires_at,
                &record.error_code,
                &record.error_message,
                &now,
            ],
        )
    }
}

#[derive(Deserialize)]
struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    iss: Option<String>,
    error: Option<String>,
}

async fn oauth_callback(
    State(authorization): State<Arc<ServerAuthorization>>,
    Query(query): Query<CallbackQuery>,
) -> impl IntoResponse {
    let Some(state) = query.state else {
        return (StatusCode::BAD_REQUEST, "Missing OAuth state").into_response();
    };
    let Some(attempt) = authorization.attempts.lock().await.remove(&state) else {
        return (StatusCode::BAD_REQUEST, "Invalid or expired OAuth callback").into_response();
    };
    {
        let lifecycle = authorization.lifecycle(&attempt.server_id).await;
        let guard = lifecycle.lock().await;
        if authorization
            .ensure_current(
                &attempt.server_id,
                &attempt.resource,
                &guard,
                attempt.generation,
            )
            .is_err()
        {
            return (
                StatusCode::BAD_REQUEST,
                "OAuth authorization was superseded",
            )
                .into_response();
        }
    }
    if Utc::now() > attempt.expires_at {
        let _ = authorization
            .set_attempt_error(&attempt, "OAuth attempt expired".into())
            .await;
        return (StatusCode::BAD_REQUEST, "OAuth attempt expired").into_response();
    }
    if query.iss.as_deref() != Some(attempt.expected_issuer.as_str())
        && (attempt.issuer_parameter_required || query.iss.is_some())
    {
        let _ = authorization
            .set_attempt_error(&attempt, "OAuth issuer mismatch".into())
            .await;
        return (StatusCode::BAD_REQUEST, "OAuth issuer mismatch").into_response();
    }
    if query.error.is_some() {
        let _ = authorization
            .set_attempt_error(&attempt, "Authorization was denied".into())
            .await;
        return (StatusCode::BAD_REQUEST, "Authorization was denied").into_response();
    }
    let Some(code) = query.code else {
        return (StatusCode::BAD_REQUEST, "Missing authorization code").into_response();
    };
    let mut form = HashMap::from([
        ("grant_type".to_string(), "authorization_code".to_string()),
        ("code".to_string(), code),
        ("redirect_uri".to_string(), attempt.redirect_uri.clone()),
        ("client_id".to_string(), attempt.client_id.clone()),
        ("code_verifier".to_string(), attempt.code_verifier.clone()),
        ("resource".to_string(), attempt.resource.clone()),
    ]);
    if let Some(secret) = &attempt.client_secret {
        form.insert("client_secret".to_string(), secret.clone());
    }
    let response = authorization
        .client
        .post(&attempt.token_endpoint)
        .form(&form)
        .send()
        .await;
    let response = match response {
        Ok(response) if response.status().is_success() => response,
        Ok(response) => {
            let status = response.status();
            let _ = authorization
                .set_attempt_error(&attempt, format!("Token exchange failed: {status}"))
                .await;
            return (StatusCode::BAD_REQUEST, "Token exchange failed").into_response();
        }
        Err(_) => {
            let _ = authorization
                .set_attempt_error(&attempt, "Token endpoint is unavailable".into())
                .await;
            return (StatusCode::BAD_REQUEST, "Token endpoint is unavailable").into_response();
        }
    };
    let token = match response.json::<TokenResponse>().await {
        Ok(token) if !token.access_token.is_empty() => token,
        _ => {
            let _ = authorization
                .set_attempt_error(&attempt, "Invalid token response".into())
                .await;
            return (StatusCode::BAD_REQUEST, "Invalid token response").into_response();
        }
    };
    let lifecycle = authorization.lifecycle(&attempt.server_id).await;
    let mut guard = lifecycle.lock().await;
    if authorization
        .ensure_current(
            &attempt.server_id,
            &attempt.resource,
            &guard,
            attempt.generation,
        )
        .is_err()
    {
        return (
            StatusCode::BAD_REQUEST,
            "OAuth authorization was superseded",
        )
            .into_response();
    }
    let mut record = match authorization.load(&attempt.server_id) {
        Ok(Some(record)) => record,
        _ => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Authorization record not found",
            )
                .into_response()
        }
    };
    let access_ref = access_token_ref(&attempt.server_id);
    let refresh_ref = refresh_token_ref(&attempt.server_id);
    let previous_refresh = if token.refresh_token.is_none() {
        record.refresh_token_ref.clone()
    } else {
        record.refresh_token_ref = Some(refresh_ref.clone());
        None
    };
    record.access_token_ref = Some(access_ref.clone());
    if let Err(error) = authorization.save(&attempt.server_id, &record) {
        return (StatusCode::INTERNAL_SERVER_ERROR, error).into_response();
    }
    let write_result = (|| -> Result<(), String> {
        if let Some(reference) = previous_refresh.as_deref() {
            authorization.store.delete(reference)?;
        }
        authorization.store.set(&access_ref, &token.access_token)?;
        if let Some(refresh) = &token.refresh_token {
            authorization.store.set(&refresh_ref, refresh)?;
        }
        Ok(())
    })();
    if let Err(error) = write_result {
        guard.disabled = true;
        record.status = "required".into();
        record.set_error(Some(error.clone()));
        let _ = authorization.save(&attempt.server_id, &record);
        let _ = authorization.emit_snapshot(&attempt.server_id).await;
        return (StatusCode::INTERNAL_SERVER_ERROR, error).into_response();
    }
    record.status = "authorized".into();
    record.issuer = Some(attempt.expected_issuer);
    record.resource = Some(attempt.resource);
    record.token_endpoint = Some(attempt.token_endpoint);
    record.revocation_endpoint = attempt.revocation_endpoint;
    record.scopes = token
        .scope
        .map(|scope| scope.split_whitespace().map(str::to_string).collect())
        .unwrap_or(attempt.scopes);
    record.access_token_ref = Some(access_ref);
    record.refresh_token_ref = token.refresh_token.map(|_| refresh_ref);
    record.expires_at = token
        .expires_in
        .map(|seconds| (Utc::now() + ChronoDuration::seconds(seconds)).to_rfc3339());
    record.set_error(None);
    if let Err(error) = authorization.save(&attempt.server_id, &record) {
        return (StatusCode::INTERNAL_SERVER_ERROR, error).into_response();
    }
    guard.disabled = false;
    let _ = authorization.emit_snapshot(&attempt.server_id).await;
    (
        StatusCode::OK,
        "Authorization complete. You can return to Moor.",
    )
        .into_response()
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    scope: Option<String>,
}

fn authorization_error_code(message: &str) -> &'static str {
    if message.starts_with("MCP authorization discovery failed:") {
        return "discovery_failed";
    }
    if message.starts_with("Failed to bind OAuth callback:") {
        return "callback_invalid";
    }
    if message.starts_with("Token exchange failed:") {
        return "token_exchange_failed";
    }
    match message {
        "Authorization server does not support PKCE S256" => "pkce_unsupported",
        "Invalid MCP Server URL"
        | "Protected resource metadata has no authorization servers"
        | "Invalid resource metadata URL"
        | "Invalid protected resource metadata"
        | "Protected resource metadata is unavailable"
        | "Invalid authorization server issuer"
        | "Authorization server issuer mismatch"
        | "Invalid authorization server metadata"
        | "Authorization server metadata is unavailable"
        | "Invalid authorization endpoint"
        | "Invalid token endpoint"
        | "OAuth endpoints must use HTTPS" => "discovery_failed",
        "Register an OAuth client before authorizing"
        | "Invalid client registration endpoint"
        | "Dynamic client registration failed"
        | "Dynamic client registration was rejected"
        | "Invalid dynamic client registration response"
        | "Dynamic client registration returned no client ID"
        | "OAuth client registration is unavailable" => "registration_failed",
        "OAuth issuer mismatch" | "Authorization was denied" => "callback_invalid",
        "Invalid token response" | "Invalid OAuth refresh response" | "OAuth expiry is invalid" => {
            "token_exchange_failed"
        }
        "OAuth token endpoint is unavailable"
        | "OAuth token endpoint is unavailable; reauthorize the Server"
        | "Token endpoint is unavailable" => "token_endpoint_unavailable",
        "System credential store is unavailable" => "credential_store_unavailable",
        "OAuth refresh token is unavailable"
        | "OAuth refresh was rejected; reauthorize the Server" => "refresh_rejected",
        "Additional OAuth scopes are required" => "insufficient_scope",
        _ => "authorization_failed",
    }
}

fn random_url_safe(bytes: usize) -> Result<String, String> {
    let mut buffer = vec![0u8; bytes];
    getrandom::getrandom(&mut buffer).map_err(|error| error.to_string())?;
    Ok(URL_SAFE_NO_PAD.encode(buffer))
}

fn validate_oauth_http_url(url: &Url) -> Result<(), String> {
    if url.scheme() != "https"
        && !url
            .host_str()
            .is_some_and(|host| matches!(host, "localhost" | "127.0.0.1" | "::1"))
    {
        return Err("OAuth endpoints must use HTTPS".into());
    }
    Ok(())
}

pub(crate) fn canonical_resource(url: &Url) -> Result<String, String> {
    validate_oauth_http_url(url)?;
    let mut canonical = url.clone();
    canonical.set_fragment(None);
    canonical.set_query(None);
    Ok(canonical.to_string().trim_end_matches('/').to_string())
}

fn protected_resource_candidates(url: &Url) -> Result<Vec<Url>, String> {
    let path = url.path();
    let mut endpoint = url.clone();
    endpoint.set_path(&format!("/.well-known/oauth-protected-resource{path}"));
    endpoint.set_query(None);
    endpoint.set_fragment(None);
    let mut root = url.clone();
    root.set_path("/.well-known/oauth-protected-resource");
    root.set_query(None);
    root.set_fragment(None);
    Ok(vec![endpoint, root])
}

fn authorization_metadata_candidates(issuer: &Url) -> Result<Vec<Url>, String> {
    let candidate = |path: String| {
        let mut url = issuer.clone();
        url.set_path(&path);
        url.set_query(None);
        url.set_fragment(None);
        url
    };
    let issuer_path = issuer.path();
    if issuer_path.is_empty() || issuer_path == "/" {
        return Ok(vec![
            candidate("/.well-known/oauth-authorization-server".into()),
            candidate("/.well-known/openid-configuration".into()),
        ]);
    }
    Ok(vec![
        candidate(format!(
            "/.well-known/oauth-authorization-server{issuer_path}"
        )),
        candidate(format!("/.well-known/openid-configuration{issuer_path}")),
        candidate(format!("{issuer_path}/.well-known/openid-configuration")),
    ])
}

fn challenge_parameter(challenge: &str, name: &str) -> Option<String> {
    for part in challenge.split(',') {
        let part = part.trim();
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        let key = key.rsplit_once(' ').map(|(_, key)| key).unwrap_or(key);
        if key.trim().eq_ignore_ascii_case(name) {
            return Some(value.trim().trim_matches('"').to_string());
        }
    }
    None
}

fn access_token_ref(server_id: &str) -> String {
    format!("server:{server_id}:oauth:access")
}

fn refresh_token_ref(server_id: &str) -> String {
    format!("server:{server_id}:oauth:refresh")
}

fn client_secret_ref(server_id: &str) -> String {
    format!("server:{server_id}:oauth:client-secret")
}
