// User interaction tracking — kept for future use by analytics circuits.
#![allow(dead_code)]
use crate::db::BrainDb;
use crate::error::BrainError;
use rusqlite::params;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInteraction {
    pub action: String,
    pub node_id: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub created_at: String,
}

/// Track a user interaction (lightweight, fire-and-forget)
pub async fn track(db: &BrainDb, action: &str, node_id: Option<&str>) {
    let now = chrono::Utc::now().to_rfc3339();
    let action_owned = action.to_string();
    let node_owned = node_id.map(|s| s.to_string());

    let _ = db.with_conn(move |conn| -> Result<(), BrainError> {
        let id = format!("user_interaction:{}", uuid::Uuid::now_v7());
        conn.execute(
            "INSERT INTO user_interaction (id, action, node_id, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![id, action_owned, node_owned, now],
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(())
    }).await;
}

/// Get recent interactions (for profile synthesis)
pub async fn get_recent(db: &BrainDb, limit: u64) -> Vec<UserInteraction> {
    let lim = limit;
    db.with_conn(move |conn| -> Result<Vec<UserInteraction>, BrainError> {
        let mut stmt = conn.prepare(
            "SELECT action, node_id, metadata, created_at \
             FROM user_interaction ORDER BY created_at DESC LIMIT ?1"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map(params![lim], |row| {
            let metadata_json: Option<String> = row.get(2)?;
            Ok(UserInteraction {
                action: row.get(0)?,
                node_id: row.get(1)?,
                metadata: metadata_json.and_then(|s| serde_json::from_str(&s).ok()),
                created_at: row.get(3)?,
            })
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(n) = r { result.push(n); } }
        Ok(result)
    }).await.unwrap_or_default()
}
