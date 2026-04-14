use crate::db::BrainDb;
use crate::error::BrainError;
use rusqlite::params;

/// Calculate quality scores in batches.
pub async fn calculate_quality_scores(db: &BrainDb) -> Result<(u64, u64), BrainError> {
    // Pre-compute edge counts with aggregate query
    let edge_counts: std::collections::HashMap<String, u64> = db.with_conn(|conn| {
        let mut counts: std::collections::HashMap<String, u64> = std::collections::HashMap::new();

        let mut stmt = conn.prepare(
            "SELECT source_id, COUNT(*) FROM edges GROUP BY source_id"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?))
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        for r in rows { if let Ok((id, c)) = r { *counts.entry(id).or_insert(0) += c; } }

        let mut stmt2 = conn.prepare(
            "SELECT target_id, COUNT(*) FROM edges GROUP BY target_id"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows2 = stmt2.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?))
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        for r in rows2 { if let Ok((id, c)) = r { *counts.entry(id).or_insert(0) += c; } }

        Ok(counts)
    }).await?;

    const NODES_PER_CYCLE: u64 = 10000;

    let total: u64 = db.with_conn(|conn| {
        conn.query_row("SELECT COUNT(*) FROM nodes", [], |row| row.get(0))
            .map_err(|e| BrainError::Database(e.to_string()))
    }).await?;

    let max_offset = total.saturating_sub(NODES_PER_CYCLE);
    let start_offset = if max_offset > 0 {
        (chrono::Utc::now().timestamp() as u64).wrapping_mul(2654435761) % (max_offset + 1)
    } else { 0 };

    let (updated, failed) = db.with_conn(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT n.id, LENGTH(n.content), n.source_type, n.tags, n.summary, \
             CASE WHEN e.node_id IS NOT NULL THEN 1 ELSE 0 END AS has_embedding \
             FROM nodes n LEFT JOIN embeddings e ON e.node_id = n.id \
             LIMIT ?1 OFFSET ?2"
        ).map_err(|e| BrainError::Database(e.to_string()))?;

        let rows: Vec<(String, u64, String, String, String, bool)> = stmt.query_map(
            params![NODES_PER_CYCLE, start_offset],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, u64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, bool>(5)?,
                ))
            }
        ).map_err(|e| BrainError::Database(e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();

        let mut updated = 0u64;
        let mut failed = 0u64;

        for (id, content_len, source_type, tags_json, summary, has_embedding) in &rows {
            let tags: Vec<String> = serde_json::from_str(tags_json).unwrap_or_default();

            let len_score = (*content_len as f64 / 3000.0).min(1.0).sqrt() * 0.20;

            let source_score = match source_type.as_str() {
                "research" => 0.15,
                "web" => 0.14,
                "manual" | "ai_memory" | "ubs_vault" => 0.13,
                "project" | "file" => 0.12,
                "auto_sync" => 0.11,
                "chat_history" => 0.10,
                _ => 0.11,
            };

            let conn_count = edge_counts.get(id).copied().unwrap_or(0) as f64;
            let conn_score = (conn_count.log2().max(0.0) / 4.0).min(1.0) * 0.20;

            let emb_score = if *has_embedding { 0.15 } else { 0.0 };
            let tag_score = (tags.len() as f64 / 5.0).min(1.0) * 0.15;

            let summary_score = if !summary.ends_with("...") && summary.len() > 50 {
                0.15
            } else if !summary.ends_with("...") && summary.len() > 20 {
                0.10
            } else if summary.len() > 100 {
                0.07
            } else {
                0.03
            };

            let quality = (len_score + source_score + conn_score + emb_score + tag_score + summary_score).clamp(0.0, 1.0);

            match conn.execute(
                "UPDATE nodes SET quality_score = ?1 WHERE id = ?2",
                params![quality, id],
            ) {
                Ok(_) => updated += 1,
                Err(_) => failed += 1,
            }
        }

        Ok((updated, failed))
    }).await?;

    log::info!("Quality scoring (sampled cycle): {} updated, {} failed (offset={}, total={})", updated, failed, start_offset, total);
    Ok((updated, failed))
}
