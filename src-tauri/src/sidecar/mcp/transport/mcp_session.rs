use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::Instant;

/// Safety cap for a local single-user gateway; hitting it signals a leak or
/// client bug, so new sessions are rejected (503) instead of evicting.
const MAX_SESSIONS: usize = 128;

/// Tracks Streamable HTTP MCP sessions with idle-TTL expiry.
pub struct McpSessionStore {
    sessions: RwLock<HashMap<String, Instant>>,
}

impl McpSessionStore {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
        }
    }

    /// Creates a session; `None` means the store is at capacity.
    pub async fn create(&self) -> Option<String> {
        let mut sessions = self.sessions.write().await;
        if sessions.len() >= MAX_SESSIONS {
            return None;
        }
        let id = uuid::Uuid::new_v4().to_string();
        sessions.insert(id.clone(), Instant::now());
        Some(id)
    }

    /// Returns true (refreshing liveness) when the session exists and has been
    /// idle for less than `ttl`; expired entries are dropped on sight.
    pub async fn validate_and_touch(&self, session_id: &str, ttl: Duration) -> bool {
        let mut sessions = self.sessions.write().await;
        match sessions.get_mut(session_id) {
            Some(last_active) if last_active.elapsed() <= ttl => {
                *last_active = Instant::now();
                true
            }
            Some(_) => {
                sessions.remove(session_id);
                false
            }
            None => false,
        }
    }

    pub async fn remove(&self, session_id: &str) -> bool {
        self.sessions.write().await.remove(session_id).is_some()
    }

    /// Drops sessions idle beyond `ttl`; returns how many were removed.
    pub async fn sweep_expired(&self, ttl: Duration) -> usize {
        let mut sessions = self.sessions.write().await;
        let before = sessions.len();
        sessions.retain(|_, last_active| last_active.elapsed() <= ttl);
        before - sessions.len()
    }

    #[cfg(test)]
    pub async fn len(&self) -> usize {
        self.sessions.read().await.len()
    }
}

impl Default for McpSessionStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_TTL: Duration = Duration::from_secs(60 * 60);

    #[tokio::test]
    async fn create_validate_and_remove_session() {
        let store = McpSessionStore::new();
        let id = store.create().await.expect("session created");
        assert!(
            store
                .validate_and_touch(&id, TEST_TTL)
                .await
        );
        assert!(store.remove(&id).await);
        assert!(
            !store
                .validate_and_touch(&id, TEST_TTL)
                .await
        );
    }

    #[tokio::test(start_paused = true)]
    async fn expired_session_fails_validation_and_is_dropped() {
        let store = McpSessionStore::new();
        let id = store.create().await.expect("session created");
        tokio::time::advance(TEST_TTL + Duration::from_secs(1)).await;
        assert!(
            !store
                .validate_and_touch(&id, TEST_TTL)
                .await
        );
        assert_eq!(store.len().await, 0);
    }

    #[tokio::test(start_paused = true)]
    async fn sweep_expired_removes_only_idle_sessions() {
        let store = McpSessionStore::new();
        let idle = store.create().await.expect("session created");
        store
            .create()
            .await
            .expect("second session created");
        tokio::time::advance(TEST_TTL + Duration::from_secs(1)).await;
        // Refresh the second session so only the first one is idle.
        let mut sessions = store.sessions.write().await;
        let keys: Vec<String> = sessions.keys().cloned().collect();
        for key in keys {
            if key != idle {
                sessions.insert(key, Instant::now());
            }
        }
        drop(sessions);

        assert_eq!(store.sweep_expired(TEST_TTL).await, 1);
        assert_eq!(store.len().await, 1);
    }

    #[tokio::test]
    async fn create_returns_none_at_capacity() {
        let store = McpSessionStore::new();
        for _ in 0..MAX_SESSIONS {
            store
                .create()
                .await
                .expect("sessions below capacity should be created");
        }
        assert!(store.create().await.is_none());
        assert_eq!(store.len().await, MAX_SESSIONS);
    }
}
