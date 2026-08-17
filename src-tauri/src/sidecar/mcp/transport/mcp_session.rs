use std::collections::HashSet;
use tokio::sync::RwLock;

/// Tracks Streamable HTTP MCP sessions for GET SSE streams.
pub struct McpSessionStore {
    sessions: RwLock<HashSet<String>>,
}

impl McpSessionStore {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashSet::new()),
        }
    }

    pub async fn create(&self) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        self.sessions.write().await.insert(id.clone());
        id
    }

    pub async fn contains(&self, session_id: &str) -> bool {
        self.sessions.read().await.contains(session_id)
    }

    pub async fn remove(&self, session_id: &str) -> bool {
        self.sessions.write().await.remove(session_id)
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

    #[tokio::test]
    async fn create_contains_and_remove_session() {
        let store = McpSessionStore::new();
        let id = store.create().await;
        assert!(store.contains(&id).await);
        assert!(store.remove(&id).await);
        assert!(!store.contains(&id).await);
    }
}
