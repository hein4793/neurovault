//! Tiered Memory — stamps every node with memory_tier = "hot" | "warm" | "cold".

use crate::db::BrainDb;
use rusqlite::params;
use std::sync::Arc;
use std::time::Duration;

pub async fn run_tier_loop(db: Arc<BrainDb>) {
    tokio::time::sleep(Duration::from_secs(900)).await;
    log::info!("Memory tier loop started");
    loop {
        match run_one_pass(&db).await {
            Ok(stats) => {
                log::info!("Memory tier pass: {} → hot, {} → warm, {} → cold (scanned {})",
                    stats.promoted_hot, stats.promoted_warm, stats.demoted_cold, stats.scanned);
                let _ = log_pass(&db, &stats).await;
            }
            Err(e) => log::warn!("Memory tier pass failed: {}", e),
        }
        tokio::time::sleep(Duration::from_secs(21_600)).await;
    }
}

#[derive(Debug, Default)]
pub struct TierStats {
    pub scanned: u64, pub promoted_hot: u64, pub promoted_warm: u64,
    pub demoted_cold: u64, pub already_correct: u64,
}

pub struct TierRow {
    pub id: String,
    pub accessed_at: String,
    pub created_at: String,
    pub node_type: String,
    pub memory_tier: Option<String>,
    pub compression_parent: Option<String>,
}

impl TierNodeFields for TierRow {
    fn node_type(&self) -> &str { &self.node_type }
    fn compression_parent(&self) -> Option<&str> { self.compression_parent.as_deref() }
    fn accessed_at(&self) -> &str { &self.accessed_at }
    fn created_at(&self) -> &str { &self.created_at }
}

pub async fn run_one_pass(db: &BrainDb) -> Result<TierStats, String> {
    let mut stats = TierStats::default();
    let now = chrono::Utc::now();
    let seven_days_ago = (now - chrono::Duration::days(7)).to_rfc3339();
    let ninety_days_ago = (now - chrono::Duration::days(90)).to_rfc3339();
    let offset = (now.timestamp() as u64).wrapping_mul(2654435761) % 1_000_000;

    let nodes: Vec<TierRow> = db.with_conn(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, accessed_at, created_at, node_type, memory_tier, compression_parent \
             FROM nodes LIMIT 5000 OFFSET ?1"
        ).map_err(|e| crate::error::BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map(params![offset], |row| {
            Ok(TierRow {
                id: row.get(0)?, accessed_at: row.get(1)?, created_at: row.get(2)?,
                node_type: row.get(3)?, memory_tier: row.get(4)?, compression_parent: row.get(5)?,
            })
        }).map_err(|e| crate::error::BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(n) = r { result.push(n); } }
        Ok(result)
    }).await.map_err(|e| e.to_string())?;

    if nodes.is_empty() { return Ok(stats); }
    stats.scanned = nodes.len() as u64;

    let thinking_types = ["hypothesis", "insight", "decision", "strategy", "contradiction", "prediction", "synthesis", "summary_cluster"];

    let mut updates: Vec<(String, &'static str)> = Vec::new();
    for node in &nodes {
        let target_tier = compute_target_tier(node, &thinking_types, &seven_days_ago, &ninety_days_ago);
        if node.memory_tier.as_deref() == Some(target_tier) {
            stats.already_correct += 1;
            continue;
        }
        updates.push((node.id.clone(), target_tier));
        match target_tier {
            "hot" => stats.promoted_hot += 1,
            "warm" => stats.promoted_warm += 1,
            "cold" => stats.demoted_cold += 1,
            _ => {}
        }
    }

    if !updates.is_empty() {
        db.with_conn(move |conn| {
            for (id, tier) in &updates {
                let _ = conn.execute("UPDATE nodes SET memory_tier = ?1 WHERE id = ?2", params![tier, id]);
            }
            Ok(())
        }).await.map_err(|e| e.to_string())?;
    }

    Ok(stats)
}

fn compute_target_tier(
    node: &impl TierNodeFields, thinking_types: &[&str],
    seven_days_ago: &str, ninety_days_ago: &str,
) -> &'static str {
    if thinking_types.iter().any(|t| *t == node.node_type()) { return "hot"; }
    if node.compression_parent().map(|p| !p.is_empty()).unwrap_or(false) { return "warm"; }
    let timestamp = if node.accessed_at() > node.created_at() { node.accessed_at() } else { node.created_at() };
    if timestamp > seven_days_ago { "hot" } else if timestamp > ninety_days_ago { "warm" } else { "cold" }
}

trait TierNodeFields {
    fn node_type(&self) -> &str;
    fn compression_parent(&self) -> Option<&str>;
    fn accessed_at(&self) -> &str;
    fn created_at(&self) -> &str;
}

async fn log_pass(db: &BrainDb, stats: &TierStats) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    let stats_json = serde_json::to_string(&serde_json::json!({
        "scanned": stats.scanned, "hot": stats.promoted_hot,
        "warm": stats.promoted_warm, "cold": stats.demoted_cold,
    })).unwrap_or_default();
    let _ = db.with_conn(move |conn| {
        let id = format!("memory_tier_log:{}", uuid::Uuid::now_v7());
        conn.execute(
            "INSERT INTO memory_tier_log (id, stats, created_at) VALUES (?1, ?2, ?3)",
            params![id, stats_json, now],
        ).map_err(|e| crate::error::BrainError::Database(e.to_string()))
    }).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockNode { node_type: String, compression_parent: Option<String>, accessed_at: String, created_at: String }
    impl TierNodeFields for MockNode {
        fn node_type(&self) -> &str { &self.node_type }
        fn compression_parent(&self) -> Option<&str> { self.compression_parent.as_deref() }
        fn accessed_at(&self) -> &str { &self.accessed_at }
        fn created_at(&self) -> &str { &self.created_at }
    }

    #[test]
    fn thinking_nodes_are_always_hot() {
        let n = MockNode { node_type: "insight".into(), compression_parent: None, accessed_at: "2000-01-01T00:00:00Z".into(), created_at: "2000-01-01T00:00:00Z".into() };
        let thinking = ["hypothesis", "insight", "decision", "strategy", "contradiction", "prediction", "synthesis", "summary_cluster"];
        let r = (chrono::Utc::now() - chrono::Duration::days(7)).to_rfc3339();
        let o = (chrono::Utc::now() - chrono::Duration::days(90)).to_rfc3339();
        assert_eq!(compute_target_tier(&n, &thinking, &r, &o), "hot");
    }

    #[test]
    fn recent_node_is_hot() {
        let now = chrono::Utc::now().to_rfc3339();
        let n = MockNode { node_type: "reference".into(), compression_parent: None, accessed_at: now.clone(), created_at: now };
        let r = (chrono::Utc::now() - chrono::Duration::days(7)).to_rfc3339();
        let o = (chrono::Utc::now() - chrono::Duration::days(90)).to_rfc3339();
        assert_eq!(compute_target_tier(&n, &["insight"], &r, &o), "hot");
    }

    #[test]
    fn old_node_is_cold() {
        let n = MockNode { node_type: "reference".into(), compression_parent: None, accessed_at: "2000-01-01T00:00:00Z".into(), created_at: "2000-01-01T00:00:00Z".into() };
        let r = (chrono::Utc::now() - chrono::Duration::days(7)).to_rfc3339();
        let o = (chrono::Utc::now() - chrono::Duration::days(90)).to_rfc3339();
        assert_eq!(compute_target_tier(&n, &["insight"], &r, &o), "cold");
    }

    #[test]
    fn compressed_node_is_warm() {
        let now = chrono::Utc::now().to_rfc3339();
        let n = MockNode { node_type: "reference".into(), compression_parent: Some("node:summary_xyz".into()), accessed_at: now.clone(), created_at: now };
        let r = (chrono::Utc::now() - chrono::Duration::days(7)).to_rfc3339();
        let o = (chrono::Utc::now() - chrono::Duration::days(90)).to_rfc3339();
        assert_eq!(compute_target_tier(&n, &["insight"], &r, &o), "warm");
    }
}
