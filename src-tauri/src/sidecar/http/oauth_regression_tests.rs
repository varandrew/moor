use super::{create_app, AppState};
use crate::sidecar::services::server_authorization::CredentialStore;
use axum::http::StatusCode;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;

#[test]
fn oauth_credentials_use_a_platform_store() {
    let entry = keyring::Entry::new("com.snowautumn.moor.test", "feature-check").unwrap();
    assert!(!entry.get_credential().is::<keyring::mock::MockCredential>());
}

#[derive(Default)]
struct TestCredentialStore {
    values: Mutex<HashMap<String, String>>,
    fail_delete: AtomicBool,
}

impl CredentialStore for TestCredentialStore {
    fn get(&self, reference: &str) -> Result<Option<String>, String> {
        Ok(self.values.lock().unwrap().get(reference).cloned())
    }

    fn set(&self, reference: &str, value: &str) -> Result<(), String> {
        self.values
            .lock()
            .unwrap()
            .insert(reference.into(), value.into());
        Ok(())
    }

    fn delete(&self, reference: &str) -> Result<(), String> {
        if self.fail_delete.load(Ordering::SeqCst) {
            return Err("System credential store is unavailable".into());
        }
        self.values.lock().unwrap().remove(reference);
        Ok(())
    }
}

#[tokio::test]
async fn credential_cleanup_failure_preserves_server_for_retry() {
    let data_dir = std::env::temp_dir().join(format!("moor-cleanup-{}", uuid::Uuid::new_v4()));
    let store = Arc::new(TestCredentialStore::default());
    let state = AppState::for_test_with_credential_store(&data_dir, store.clone());
    crate::sidecar::db::profile_repo::ProfileRepository::new(&state.db)
        .seed_default()
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let gateway = format!("http://{}", listener.local_addr().unwrap());
    let server =
        tokio::spawn(async move { axum::serve(listener, create_app(state)).await.unwrap() });
    let client = reqwest::Client::new();
    let created = client
        .post(format!("{gateway}/api/servers"))
        .header("x-moor-token", "test-token")
        .json(&json!({ "name": "protected", "connectionType": "http", "url": "http://127.0.0.1/mcp" }))
        .send().await.unwrap().json::<Value>().await.unwrap();
    let id = created["id"].as_str().unwrap();
    let registered = client
        .put(format!("{gateway}/api/servers/{id}/authorization/client"))
        .header("x-moor-token", "test-token")
        .json(&json!({ "clientId": "client", "clientSecret": "secret" }))
        .send()
        .await
        .unwrap();
    assert_eq!(registered.status(), StatusCode::OK);

    store.fail_delete.store(true, Ordering::SeqCst);
    let changed = client
        .put(format!("{gateway}/api/servers/{id}"))
        .header("x-moor-token", "test-token")
        .json(&json!({ "url": "http://127.0.0.1/new-mcp" }))
        .send()
        .await
        .unwrap();
    assert!(!changed.status().is_success());
    let unchanged = client
        .get(format!("{gateway}/api/servers/{id}"))
        .header("x-moor-token", "test-token")
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(unchanged["url"], "http://127.0.0.1/mcp");
    let deleted = client
        .delete(format!("{gateway}/api/servers/{id}"))
        .header("x-moor-token", "test-token")
        .send()
        .await
        .unwrap();
    assert!(!deleted.status().is_success());
    let error = deleted.json::<Value>().await.unwrap();
    assert_eq!(
        error["error"]["message"],
        "Failed to remove OAuth credentials. Check your system credential store and try again."
    );
    let retained = client
        .get(format!("{gateway}/api/servers/{id}"))
        .header("x-moor-token", "test-token")
        .send()
        .await
        .unwrap();
    assert_eq!(retained.status(), StatusCode::OK);

    let plain = client
        .post(format!("{gateway}/api/servers"))
        .header("x-moor-token", "test-token")
        .json(&json!({ "name": "plain", "connectionType": "http", "url": "http://127.0.0.1/mcp" }))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    let plain_id = plain["id"].as_str().unwrap();
    let deleted_plain = client
        .delete(format!("{gateway}/api/servers/{plain_id}"))
        .header("x-moor-token", "test-token")
        .send()
        .await
        .unwrap();
    assert_eq!(deleted_plain.status(), StatusCode::OK);

    store.fail_delete.store(false, Ordering::SeqCst);
    let deleted = client
        .delete(format!("{gateway}/api/servers/{id}"))
        .header("x-moor-token", "test-token")
        .send()
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::OK);
    server.abort();
    let _ = std::fs::remove_dir_all(data_dir);
}
