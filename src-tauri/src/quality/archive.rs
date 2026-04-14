//! Node archival, deduplication, and memory optimization.

use crate::db::BrainDb;
use crate::error::BrainError;
use rusqlite::params;

/// Archive nodes with very low quality AND decay to cold storage.
/// Moves up to 1000 nodes per call.
pub async fn archive_low_quality_nodes(db: &BrainDb) -> Result<u64, BrainError> {
    let ids: Vec<String> = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id FROM nodes WHERE quality_score < 0.15 AND decay_score < 0.15 \
             AND node_type NOT IN ('synthesis', 'architecture', 'core') LIMIT 1000"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(id) = r { result.push(id); } }
        Ok(result)
    }).await?;

    if ids.is_empty() {
        return Ok(0);
    }

    let archived = db.with_conn(move |conn| {
        let mut archived = 0u64;
        let now = chrono::Utc::now().to_rfc3339();
        for id in &ids {
            // Copy node data to archive as JSON
            let node_json: Result<String, _> = conn.query_row(
                "SELECT json_object('id', id, 'title', title, 'content', content, \
                 'summary', summary, 'domain', domain, 'topic', topic, 'tags', tags, \
                 'node_type', node_type, 'source_type', source_type, 'quality_score', quality_score) \
                 FROM nodes WHERE id = ?1",
                params![id],
                |row| row.get(0),
            );

            if let Ok(json) = node_json {
                let archive_id = format!("node_archive:{}", uuid::Uuid::now_v7());
                let _ = conn.execute(
                    "INSERT OR IGNORE INTO node_archive (id, node_data, archived_at) VALUES (?1, ?2, ?3)",
                    params![archive_id, json, now],
                );
            }

            // Delete from active
            let _ = conn.execute("DELETE FROM nodes WHERE id = ?1", params![id]);
            archived += 1;
        }

        // Clean orphaned edges
        let _ = conn.execute(
            "DELETE FROM edges WHERE source_id NOT IN (SELECT id FROM nodes) \
             OR target_id NOT IN (SELECT id FROM nodes)",
            [],
        );

        Ok(archived)
    }).await?;

    log::info!("Archived {} low-quality nodes", archived);
    Ok(archived)
}

/// Deduplicate nodes with identical content_hash (keep highest quality).
pub async fn auto_deduplicate(db: &BrainDb) -> Result<u64, BrainError> {
    let deduped = db.with_conn(|conn| {
        // Find duplicate content_hash groups
        let mut stmt = conn.prepare(
            "SELECT content_hash, COUNT(*) as cnt FROM nodes \
             GROUP BY content_hash HAVING cnt > 1 LIMIT 100"
        ).map_err(|e| BrainError::Database(e.to_string()))?;

        let dupes: Vec<String> = stmt.query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| BrainError::Database(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        let mut total_deduped = 0u64;

        for hash in &dupes {
            // Get all IDs for this hash, ordered by quality (keep best)
            let mut id_stmt = conn.prepare(
                "SELECT id FROM nodes WHERE content_hash = ?1 ORDER BY quality_score DESC"
            ).map_err(|e| BrainError::Database(e.to_string()))?;

            let ids: Vec<String> = id_stmt.query_map(params![hash], |row| row.get::<_, String>(0))
                .map_err(|e| BrainError::Database(e.to_string()))?
                .filter_map(|r| r.ok())
                .collect();

            // Keep first (highest quality), delete rest
            for id in ids.iter().skip(1) {
                let _ = conn.execute("DELETE FROM nodes WHERE id = ?1", params![id]);
                total_deduped += 1;
            }
        }

        if total_deduped > 0 {
            let _ = conn.execute(
                "DELETE FROM edges WHERE source_id NOT IN (SELECT id FROM nodes) \
                 OR target_id NOT IN (SELECT id FROM nodes)",
                [],
            );
        }

        Ok(total_deduped)
    }).await?;

    log::info!("Deduplicated {} nodes", deduped);
    Ok(deduped)
}

/// Strip embeddings from low-quality nodes to save memory.
pub async fn strip_low_value_embeddings(db: &BrainDb) -> Result<u64, BrainError> {
    let count = db.with_conn(|conn| {
        let deleted = conn.execute(
            "DELETE FROM embeddings WHERE node_id IN \
             (SELECT id FROM nodes WHERE quality_score < 0.25)",
            [],
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(deleted as u64)
    }).await?;

    log::info!("Stripped embeddings from low-quality nodes (~{})", count);
    Ok(count)
}
