use crate::ai::client::LlmClient;
use crate::db::BrainDb;
use crate::error::BrainError;
use rusqlite::params;

/// Generate AI summary for a single node
pub async fn summarize_node(db: &BrainDb, client: &LlmClient, node_id: &str) -> Result<String, BrainError> {
    let id = node_id.to_string();
    let content: String = db.with_conn(move |conn| {
        conn.query_row(
            "SELECT content FROM nodes WHERE id = ?1",
            params![id],
            |row| row.get(0),
        ).map_err(|e| BrainError::NotFound(format!("Node not found: {}", e)))
    }).await?;

    let summary = client.summarize(&content).await?;

    let id2 = node_id.to_string();
    let summary_clone = summary.clone();
    db.with_conn(move |conn| {
        conn.execute(
            "UPDATE nodes SET summary = ?1 WHERE id = ?2",
            params![summary_clone, id2],
        ).map_err(|e| BrainError::Database(e.to_string()))
    }).await?;

    Ok(summary)
}

/// Backfill AI summaries for nodes with truncated summaries (LIMIT 50 per batch)
pub async fn backfill_summaries(db: &BrainDb, client: &LlmClient) -> Result<(u64, u64), BrainError> {
    let nodes: Vec<(String, String, String)> = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, title, content FROM nodes \
             WHERE summary LIKE '%...' AND LENGTH(content) > 100 LIMIT 50"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(n) = r { result.push(n); } }
        Ok(result)
    }).await?;

    let mut generated = 0u64;
    let mut failed = 0u64;

    for (id, title, content) in &nodes {
        match client.summarize(content).await {
            Ok(summary) => {
                let id_clone = id.clone();
                let summary_clone = summary.clone();
                let _ = db.with_conn(move |conn| {
                    conn.execute(
                        "UPDATE nodes SET summary = ?1 WHERE id = ?2",
                        params![summary_clone, id_clone],
                    ).map_err(|e| BrainError::Database(e.to_string()))
                }).await;
                generated += 1;
            }
            Err(e) => {
                log::warn!("Failed to summarize {}: {}", title, e);
                failed += 1;
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    }

    Ok((generated, failed))
}
