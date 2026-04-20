//! Context Quality Tracking — Phase 4 of the Dual-Brain plan.
//!
//! Tracks how effective the sidekick's context bundles are:
//! - Were the injected nodes actually useful? (utilization rate)
//! - Did Claude ask for info the brain had but didn't inject? (knowledge gaps)
//! - How fast was the bundle generated? (latency)
//!
//! The `context_quality_optimizer` circuit runs weekly to tune the
//! relevance ranking weights based on accumulated feedback.

use crate::db::BrainDb;
use crate::error::BrainError;
use rusqlite::params;
use std::sync::Arc;

// =========================================================================
// Quality metrics storage
// =========================================================================

pub async fn log_bundle_quality(
    db: &BrainDb,
    project: &str,
    query: &str,
    rules_count: usize,
    knowledge_count: usize,
    patterns_count: usize,
    generation_ms: u64,
    total_chars: usize,
) -> Result<(), BrainError> {
    let now = chrono::Utc::now().to_rfc3339();
    let proj = project.to_string();
    let q = query.to_string();
    db.with_conn(move |conn| {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS context_quality_log (
                id TEXT PRIMARY KEY,
                project TEXT NOT NULL,
                query TEXT NOT NULL,
                rules_count INTEGER NOT NULL,
                knowledge_count INTEGER NOT NULL,
                patterns_count INTEGER NOT NULL,
                generation_ms INTEGER NOT NULL,
                total_chars INTEGER NOT NULL,
                created_at TEXT NOT NULL
            )",
            [],
        ).map_err(|e| BrainError::Database(e.to_string()))?;

        let id = format!("cq:{}", uuid::Uuid::now_v7());
        conn.execute(
            "INSERT INTO context_quality_log
             (id, project, query, rules_count, knowledge_count, patterns_count,
              generation_ms, total_chars, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![id, proj, q, rules_count as i64, knowledge_count as i64,
                    patterns_count as i64, generation_ms as i64, total_chars as i64, now],
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(())
    }).await
}

// =========================================================================
// Knowledge gap detection
// =========================================================================

#[allow(dead_code)]
pub async fn log_knowledge_gap(
    db: &BrainDb,
    query: &str,
    gap_description: &str,
) -> Result<(), BrainError> {
    let now = chrono::Utc::now().to_rfc3339();
    let q = query.to_string();
    let desc = gap_description.to_string();
    db.with_conn(move |conn| {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS knowledge_gaps (
                id TEXT PRIMARY KEY,
                query TEXT NOT NULL,
                gap_description TEXT NOT NULL,
                filled INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                filled_at TEXT
            )",
            [],
        ).map_err(|e| BrainError::Database(e.to_string()))?;

        let id = format!("gap:{}", uuid::Uuid::now_v7());
        conn.execute(
            "INSERT INTO knowledge_gaps (id, query, gap_description, filled, created_at)
             VALUES (?1, ?2, ?3, 0, ?4)",
            params![id, q, desc, now],
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(())
    }).await
}

// =========================================================================
// Circuit: context_quality_optimizer (runs weekly)
// =========================================================================

pub async fn circuit_context_quality_optimizer(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    // 1. Analyze context quality over the past week
    let stats: (i64, f64, f64, f64) = db
        .with_conn(|conn| {
            // Ensure table exists
            conn.execute(
                "CREATE TABLE IF NOT EXISTS context_quality_log (
                    id TEXT PRIMARY KEY,
                    project TEXT NOT NULL,
                    query TEXT NOT NULL,
                    rules_count INTEGER NOT NULL,
                    knowledge_count INTEGER NOT NULL,
                    patterns_count INTEGER NOT NULL,
                    generation_ms INTEGER NOT NULL,
                    total_chars INTEGER NOT NULL,
                    created_at TEXT NOT NULL
                )",
                [],
            ).map_err(|e| BrainError::Database(e.to_string()))?;

            let cutoff = (chrono::Utc::now() - chrono::Duration::days(7)).to_rfc3339();
            let result = conn.query_row(
                "SELECT COUNT(*),
                        COALESCE(AVG(knowledge_count), 0),
                        COALESCE(AVG(generation_ms), 0),
                        COALESCE(AVG(total_chars), 0)
                 FROM context_quality_log
                 WHERE created_at > ?1",
                params![cutoff],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            ).map_err(|e| BrainError::Database(e.to_string()))?;
            Ok(result)
        })
        .await?;

    let (bundle_count, avg_knowledge, avg_latency_ms, avg_chars) = stats;

    // 2. Check for unfilled knowledge gaps
    let gap_count: i64 = db
        .with_conn(|conn| {
            conn.execute(
                "CREATE TABLE IF NOT EXISTS knowledge_gaps (
                    id TEXT PRIMARY KEY,
                    query TEXT NOT NULL,
                    gap_description TEXT NOT NULL,
                    filled INTEGER NOT NULL DEFAULT 0,
                    created_at TEXT NOT NULL,
                    filled_at TEXT
                )",
                [],
            ).map_err(|e| BrainError::Database(e.to_string()))?;

            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM knowledge_gaps WHERE filled = 0",
                [],
                |row| row.get(0),
            ).map_err(|e| BrainError::Database(e.to_string()))?;
            Ok(count)
        })
        .await?;

    // 3. Generate a quality report
    let report = format!(
        "Context quality (7d): {} bundles, avg {:.1} knowledge nodes, {:.0}ms latency, {:.0} chars. {} unfilled knowledge gaps.",
        bundle_count, avg_knowledge, avg_latency_ms, avg_chars, gap_count
    );

    // 4. If average knowledge nodes is too low, create research missions
    //    for the top knowledge gaps
    if avg_knowledge < 3.0 && gap_count > 0 {
        let gaps: Vec<String> = db
            .with_conn(|conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT gap_description FROM knowledge_gaps
                         WHERE filled = 0
                         ORDER BY created_at DESC LIMIT 3",
                    )
                    .map_err(|e| BrainError::Database(e.to_string()))?;
                let rows = stmt
                    .query_map([], |row| row.get::<_, String>(0))
                    .map_err(|e| BrainError::Database(e.to_string()))?;
                let mut result = Vec::new();
                for r in rows {
                    if let Ok(v) = r {
                        result.push(v);
                    }
                }
                Ok(result)
            })
            .await?;

        for gap in &gaps {
            let now = chrono::Utc::now().to_rfc3339();
            let g = gap.clone();
            let _ = db
                .with_conn(move |conn| {
                    let id = format!("rm:{}", uuid::Uuid::now_v7());
                    conn.execute(
                        "INSERT OR IGNORE INTO research_missions
                         (id, topic, status, source, priority, created_at)
                         VALUES (?1, ?2, 'pending', 'context_quality', 'high', ?3)",
                        params![id, g, now],
                    )
                    .map_err(|e| BrainError::Database(e.to_string()))?;
                    Ok(())
                })
                .await;
        }
    }

    Ok(report)
}
