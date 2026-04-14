use crate::db::BrainDb;
use crate::error::BrainError;
use rusqlite::params;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchMission {
    pub id: String,
    pub topic: String,
    pub status: String,
    pub nodes_created: u64,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub summary: Option<String>,
}

pub async fn create_mission(db: &BrainDb, topic: &str) -> Result<ResearchMission, BrainError> {
    let now = chrono::Utc::now().to_rfc3339();
    let id = format!("research_missions:{}", uuid::Uuid::now_v7());
    let topic_owned = topic.to_string();
    let id_clone = id.clone();
    let now_clone = now.clone();

    db.with_conn(move |conn| {
        conn.execute(
            "INSERT INTO research_missions (id, topic, description, status, priority, created_at) \
             VALUES (?1, ?2, '', 'pending', 0, ?3)",
            params![id_clone, topic_owned, now_clone],
        ).map_err(|e| BrainError::Database(e.to_string()))
    }).await?;

    Ok(ResearchMission {
        id,
        topic: topic.to_string(),
        status: "pending".to_string(),
        nodes_created: 0,
        started_at: now,
        completed_at: None,
        summary: None,
    })
}

pub async fn get_missions(db: &BrainDb) -> Result<Vec<ResearchMission>, BrainError> {
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, topic, status, created_at, completed_at, result FROM research_missions \
             ORDER BY created_at DESC"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok(ResearchMission {
                id: row.get(0)?,
                topic: row.get(1)?,
                status: row.get(2)?,
                nodes_created: 0,
                started_at: row.get(3)?,
                completed_at: row.get(4)?,
                summary: row.get(5)?,
            })
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(m) = r { result.push(m); } }
        Ok(result)
    }).await
}
