use crate::db::BrainDb;
use crate::error::BrainError;
use rusqlite::params;

/// Calculate decay scores in batches.
pub async fn calculate_decay_scores(db: &BrainDb) -> Result<(u64, u64), BrainError> {
    let now = chrono::Utc::now();
    let half_life_days = 30.0f64;

    const NODES_PER_CYCLE: u64 = 10000;

    let total: u64 = db.with_conn(|conn| {
        conn.query_row("SELECT COUNT(*) FROM nodes", [], |row| row.get(0))
            .map_err(|e| BrainError::Database(e.to_string()))
    }).await?;

    let max_offset = total.saturating_sub(NODES_PER_CYCLE);
    let start_offset = if max_offset > 0 {
        (chrono::Utc::now().timestamp() as u64).wrapping_mul(2654435761) % (max_offset + 1)
    } else { 0 };

    let now_ts = now;
    let (updated, failed) = db.with_conn(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, quality_score, access_count, updated_at FROM nodes LIMIT ?1 OFFSET ?2"
        ).map_err(|e| BrainError::Database(e.to_string()))?;

        let rows: Vec<(String, f64, u64, String)> = stmt.query_map(
            params![NODES_PER_CYCLE, start_offset],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        ).map_err(|e| BrainError::Database(e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();

        let mut updated = 0u64;
        let mut failed = 0u64;

        for (id, quality_score, access_count, updated_at) in &rows {
            let updated_time = chrono::DateTime::parse_from_rfc3339(updated_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or(now_ts);

            let days_since = (now_ts - updated_time).num_hours() as f64 / 24.0;
            let recency_factor = 0.5 + 0.5 * (-days_since * (2.0f64.ln()) / half_life_days).exp();
            let access_bonus = (0.04 * (*access_count as f64).sqrt()).min(0.2);
            let decay_score = (quality_score * (recency_factor + access_bonus).min(1.0)).clamp(0.0, 1.0);

            match conn.execute(
                "UPDATE nodes SET decay_score = ?1 WHERE id = ?2",
                params![decay_score, id],
            ) {
                Ok(_) => updated += 1,
                Err(_) => failed += 1,
            }
        }

        Ok((updated, failed))
    }).await?;

    log::info!("Decay calculation (sampled cycle): {} updated, {} failed (offset={}, total={})", updated, failed, start_offset, total);
    Ok((updated, failed))
}
