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
    pub error_message: Option<String>,
    pub issuer: Option<String>,
    pub client_id: Option<String>,
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone)]
struct AuthorizationRecord {
    status: String,
    issuer: Option<String>,
    client_id: Option<String>,
    client_secret_ref: Option<String>,
    resource: Option<String>,
    scopes: Vec<String>,
    access_token_ref: Option<String>,
    refresh_token_ref: Option<String>,
    expires_at: Option<String>,
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
            scopes: Vec::new(),
            access_token_ref: None,
            refresh_token_ref: None,
            expires_at: None,
            error_message: None,
        }
    }
}

pub trait CredentialStore: Send + Sync {
    fn get(&self, reference: &str) -> Result<Option<String>, String>;
    fn set(&self, reference: &str, value: &str) -> Result<(), String>;
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
}

#[cfg(test)]
#[derive(Default)]
pub struct MemoryCredentialStore {
    values: StdMutex<HashMap<String, String>>,
    unavailable: bool,
}

#[cfg(test)]
impl MemoryCredentialStore {
    pub fn unavailable() -> Self {
        Self {
            values: StdMutex::new(HashMap::new()),
            unavailable: true,
        }
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
        if self.unavailable {
            return Err("System credential store is unavailable".into());
        }
        self.values
            .lock()
            .unwrap()
            .insert(reference.to_string(), value.to_string());
        Ok(())
    }
}

struct OAuthCredentialProvider {
    store: Arc<dyn CredentialStore>,
    access_token_ref: String,
}

impl HttpCredentialProvider for OAuthCredentialProvider {
    fn headers(&self) -> BoxFuture<'static, Result<HashMap<String, String>, RequestError>> {
        let store = self.store.clone();
        let reference = self.access_token_ref.clone();
        Box::pin(async move {
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
    code_challenge_methods_supported: Option<Vec<String>>,
    #[serde(default)]
    authorization_response_iss_parameter_supported: bool,
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
    expected_issuer: String,
    issuer_parameter_required: bool,
    resource: String,
    scopes: Vec<String>,
    expires_at: chrono::DateTime<Utc>,
    shutdown: oneshot::Sender<()>,
}

pub struct ServerAuthorization {
    db: Arc<Database>,
    event_bus: Arc<EventBus>,
    store: Arc<dyn CredentialStore>,
    client: reqwest::Client,
    attempts: Arc<Mutex<HashMap<String, PendingAttempt>>>,
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
        }
    }

    pub fn credential_provider(
        &self,
        server_id: &str,
    ) -> Result<Option<Arc<dyn HttpCredentialProvider>>, String> {
        let Some(record) = self.load(server_id)? else {
            return Ok(None);
        };
        if record.status != "authorized" {
            return Ok(None);
        }
        let access_token_ref = record
            .access_token_ref
            .ok_or("OAuth authorization requires a new grant")?;
        Ok(Some(Arc::new(OAuthCredentialProvider {
            store: self.store.clone(),
            access_token_ref,
        })))
    }

    pub async fn snapshot(&self, server_id: &str) -> Result<AuthorizationSnapshot, String> {
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
            record.error_message = Some("OAuth attempt expired".into());
            self.save(server_id, &record)?;
        }
        let attempts = self.attempts.lock().await;
        let record = self.load(server_id)?;
        let snapshot = self.snapshot_with_attempt(
            server_id,
            record,
            attempts
                .values()
                .any(|attempt| attempt.server_id == server_id),
        );
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
        let state = if authorizing {
            "authorizing".to_string()
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
            error_message: record.error_message,
            issuer: record.issuer,
            client_id: record.client_id,
            scopes: record.scopes,
        }
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
        let mut record = self
            .load(server_id)?
            .unwrap_or_else(AuthorizationRecord::required);
        record.status = "required".into();
        record.error_message = None;
        record.client_id = Some(client_id.trim().to_string());
        if let Some(secret) = client_secret.filter(|secret| !secret.is_empty()) {
            record.client_secret_ref = Some(client_secret_ref(server_id));
            self.store.set(&client_secret_ref(server_id), secret)?;
        }
        self.save(server_id, &record)?;
        self.emit_snapshot(server_id).await
    }

    pub async fn start(&self, server_id: &str) -> Result<Value, String> {
        let record = self
            .load(server_id)?
            .unwrap_or_else(AuthorizationRecord::required);
        let client_id = record
            .client_id
            .clone()
            .ok_or("Register an OAuth client before authorizing")?;
        let client_secret = match &record.client_secret_ref {
            Some(reference) => self.store.get(reference)?,
            None => None,
        };
        let server = ServerRepository::new(&self.db)
            .find_by_id(server_id)?
            .ok_or("Server not found")?;
        let server_url = server.url.ok_or("OAuth requires an HTTP Server URL")?;
        match self
            .start_inner(server_id, &server_url, client_id, client_secret)
            .await
        {
            Ok(result) => Ok(result),
            Err(error) => {
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
        client_id: String,
        client_secret: Option<String>,
    ) -> Result<Value, String> {
        let discovery = self.discover(server_url).await?;
        validate_oauth_http_url(
            &Url::parse(&discovery.metadata.authorization_endpoint)
                .map_err(|_| "Invalid authorization endpoint")?,
        )?;
        validate_oauth_http_url(
            &Url::parse(&discovery.metadata.token_endpoint)
                .map_err(|_| "Invalid token endpoint")?,
        )?;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|error| format!("Failed to bind OAuth callback: {error}"))?;
        let callback_addr = listener.local_addr().map_err(|error| error.to_string())?;
        let redirect_uri = format!("http://{callback_addr}/oauth/callback");
        let state = random_url_safe(32)?;
        let code_verifier = random_url_safe(48)?;
        let code_challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(code_verifier.as_bytes()));
        let expires_at = Utc::now() + ChronoDuration::seconds(ATTEMPT_TTL_SECONDS);
        let (shutdown, stopped) = oneshot::channel();
        {
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
                    expected_issuer: discovery.metadata.issuer.clone(),
                    issuer_parameter_required: discovery
                        .metadata
                        .authorization_response_iss_parameter_supported,
                    resource: discovery.resource.clone(),
                    scopes: discovery.scopes.clone(),
                    expires_at,
                    shutdown,
                },
            );
        }
        self.emit_snapshot(server_id).await?;
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
        })
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
        record.error_message = error_message;
        self.save(server_id, &record)?;
        self.emit_snapshot(server_id).await.map(|_| ())
    }

    async fn emit_snapshot(&self, server_id: &str) -> Result<AuthorizationSnapshot, String> {
        let snapshot = self.snapshot(server_id).await?;
        self.event_bus.emit(Evt::ServerAuthorization {
            snapshot: snapshot.clone(),
        });
        Ok(snapshot)
    }

    fn load(&self, server_id: &str) -> Result<Option<AuthorizationRecord>, String> {
        self.db.query_one(
            "SELECT status, issuer, client_id, client_secret_ref, resource, scopes, access_token_ref, refresh_token_ref, expires_at, error_message FROM server_authorizations WHERE server_id = ?1",
            &[&server_id],
            |row| {
                let scopes: String = row.get(5)?;
                Ok(AuthorizationRecord {
                    status: row.get(0)?,
                    issuer: row.get(1)?,
                    client_id: row.get(2)?,
                    client_secret_ref: row.get(3)?,
                    resource: row.get(4)?,
                    scopes: serde_json::from_str(&scopes).unwrap_or_default(),
                    access_token_ref: row.get(6)?,
                    refresh_token_ref: row.get(7)?,
                    expires_at: row.get(8)?,
                    error_message: row.get(9)?,
                })
            },
        )
    }

    fn save(&self, server_id: &str, record: &AuthorizationRecord) -> Result<(), String> {
        let scopes = serde_json::to_string(&record.scopes).map_err(|error| error.to_string())?;
        let now = Utc::now().to_rfc3339();
        self.db.run(
            "INSERT INTO server_authorizations (server_id, status, issuer, client_id, client_secret_ref, resource, scopes, access_token_ref, refresh_token_ref, expires_at, error_message, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12) ON CONFLICT(server_id) DO UPDATE SET status = excluded.status, issuer = excluded.issuer, client_id = excluded.client_id, client_secret_ref = excluded.client_secret_ref, resource = excluded.resource, scopes = excluded.scopes, access_token_ref = excluded.access_token_ref, refresh_token_ref = excluded.refresh_token_ref, expires_at = excluded.expires_at, error_message = excluded.error_message, updated_at = excluded.updated_at",
            &[
                &server_id,
                &record.status,
                &record.issuer,
                &record.client_id,
                &record.client_secret_ref,
                &record.resource,
                &scopes,
                &record.access_token_ref,
                &record.refresh_token_ref,
                &record.expires_at,
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
    if Utc::now() > attempt.expires_at {
        let _ = authorization
            .set_status(
                &attempt.server_id,
                "error",
                Some("OAuth attempt expired".into()),
            )
            .await;
        return (StatusCode::BAD_REQUEST, "OAuth attempt expired").into_response();
    }
    if query.iss.as_deref() != Some(attempt.expected_issuer.as_str())
        && (attempt.issuer_parameter_required || query.iss.is_some())
    {
        let _ = authorization
            .set_status(
                &attempt.server_id,
                "error",
                Some("OAuth issuer mismatch".into()),
            )
            .await;
        return (StatusCode::BAD_REQUEST, "OAuth issuer mismatch").into_response();
    }
    if query.error.is_some() {
        let _ = authorization
            .set_status(
                &attempt.server_id,
                "error",
                Some("Authorization was denied".into()),
            )
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
                .set_status(
                    &attempt.server_id,
                    "error",
                    Some(format!("Token exchange failed: {status}")),
                )
                .await;
            return (StatusCode::BAD_REQUEST, "Token exchange failed").into_response();
        }
        Err(_) => {
            let _ = authorization
                .set_status(
                    &attempt.server_id,
                    "error",
                    Some("Token endpoint is unavailable".into()),
                )
                .await;
            return (StatusCode::BAD_REQUEST, "Token endpoint is unavailable").into_response();
        }
    };
    let token = match response.json::<TokenResponse>().await {
        Ok(token) if !token.access_token.is_empty() => token,
        _ => {
            let _ = authorization
                .set_status(
                    &attempt.server_id,
                    "error",
                    Some("Invalid token response".into()),
                )
                .await;
            return (StatusCode::BAD_REQUEST, "Invalid token response").into_response();
        }
    };
    let access_ref = access_token_ref(&attempt.server_id);
    let refresh_ref = refresh_token_ref(&attempt.server_id);
    if let Err(error) = authorization.store.set(&access_ref, &token.access_token) {
        let _ = authorization
            .set_status(&attempt.server_id, "error", Some(error.clone()))
            .await;
        return (StatusCode::INTERNAL_SERVER_ERROR, error).into_response();
    }
    if let Some(refresh) = &token.refresh_token {
        if let Err(error) = authorization.store.set(&refresh_ref, refresh) {
            let _ = authorization
                .set_status(&attempt.server_id, "error", Some(error.clone()))
                .await;
            return (StatusCode::INTERNAL_SERVER_ERROR, error).into_response();
        }
    }
    let mut record = match authorization.load(&attempt.server_id) {
        Ok(Some(record)) => record,
        _ => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Authorization record not found",
            )
                .into_response();
        }
    };
    record.status = "authorized".into();
    record.issuer = Some(attempt.expected_issuer);
    record.resource = Some(attempt.resource);
    record.scopes = token
        .scope
        .map(|scope| scope.split_whitespace().map(str::to_string).collect())
        .unwrap_or(attempt.scopes);
    record.access_token_ref = Some(access_ref);
    record.refresh_token_ref = token.refresh_token.map(|_| refresh_ref);
    record.expires_at = token
        .expires_in
        .map(|seconds| (Utc::now() + ChronoDuration::seconds(seconds)).to_rfc3339());
    record.error_message = None;
    if let Err(error) = authorization.save(&attempt.server_id, &record) {
        return (StatusCode::INTERNAL_SERVER_ERROR, error).into_response();
    }
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

fn canonical_resource(url: &Url) -> Result<String, String> {
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
    let mut endpoint = issuer.clone();
    endpoint.set_path(&format!(
        "/.well-known/oauth-authorization-server{}",
        issuer.path()
    ));
    endpoint.set_query(None);
    endpoint.set_fragment(None);
    let mut root = issuer.clone();
    root.set_path("/.well-known/oauth-authorization-server");
    root.set_query(None);
    root.set_fragment(None);
    Ok(vec![endpoint, root])
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
