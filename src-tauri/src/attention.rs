//! Phase Omega Part IX — Attention Mechanism
//!
//! Scores all nodes by a weighted attention formula and maintains a "focus
//! window" of the top 100 most relevant nodes. The attention score combines
//! recency, quality, access frequency, domain alignment with the current
//! project, and user interest signals.

use crate::db::BrainDb;
use crate::error::BrainError;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// =========================================================================
// DATA STRUCTURES
// =========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttentionWindow {
    pub focus_nodes: Vec<FocusNode>,
    pub current_project: String,
    pub active_domains: Vec<String>,
    pub last_updated: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FocusNode {
    pub id: String,
    pub title: String,
    pub attention_score: f32,
    pub reason: String,
}

// =========================================================================
// COMPUTE ATTENTION
// =========================================================================

/// Score all recent/relevant nodes using a weighted attention formula:
///   attention = relevance_to_current_task * 0.35 +
///               recency * 0.20 +
///               quality_score * 0.15 +
///               access_frequency * 0.10 +
///               connection_to_active_project * 0.10 +
///               user_interest_alignment * 0.10
///
/// Persists the top 100 to the `attention_focus` table.
pub async fn compute_attention(db: &Arc<BrainDb>) -> Result<AttentionWindow, BrainError> {
    let now = chrono::Utc::now().to_rfc3339();

    // Detect current project: most common domain in last 24h
    let current_project: String = db
        .with_conn(|conn| {
            let project: String = conn
                .query_row(
                    "SELECT domain FROM nodes \
                     WHERE created_at >= DATETIME('now', '-1 day') AND domain != '' \
                     GROUP BY domain \
                     ORDER BY COUNT(*) DESC \
                     LIMIT 1",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or_else(|_| "general".to_string());
            Ok(project)
        })
        .await?;

    // Active domains: top 5 domains by recent activity
    let active_domains: Vec<String> = db
        .with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT domain FROM nodes \
                     WHERE created_at >= DATETIME('now', '-7 days') AND domain != '' \
                     GROUP BY domain \
                     ORDER BY COUNT(*) DESC \
                     LIMIT 5",
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let mut results = Vec::new();
            for row in rows {
                if let Ok(r) = row {
                    results.push(r);
                }
            }
            Ok(results)
        })
        .await?;

    // Get max access_count for normalization
    let max_access: f32 = db
        .with_conn(|conn| {
            let m: f64 = conn
                .query_row(
                    "SELECT COALESCE(MAX(access_count), 1) FROM nodes",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(1.0);
            Ok(m.max(1.0) as f32)
        })
        .await?;

    // User interest domains from user_cognition rules
    let user_interest_domains: Vec<String> = db
        .with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT DISTINCT n.domain \
                     FROM user_cognition uc \
                     JOIN nodes n ON n.id IN ( \
                         SELECT value FROM json_each(uc.linked_to_nodes) \
                     ) \
                     WHERE uc.confidence > 0.5 \
                     LIMIT 10",
                )
                .map_err(|e| BrainError::Database(e.to_string()));
            match stmt {
                Ok(ref mut s) => {
                    let rows = s
                        .query_map([], |row| row.get::<_, String>(0))
                        .map_err(|e| BrainError::Database(e.to_string()))?;
                    let mut results = Vec::new();
                    for row in rows {
                        if let Ok(r) = row {
                            results.push(r);
                        }
                    }
                    Ok(results)
                }
                Err(_) => Ok(Vec::new()),
            }
        })
        .await?;

    // Score candidates — sample recent + high-quality + frequently accessed nodes
    let project_clone = current_project.clone();
    let active_clone = active_domains.clone();
    let interest_clone = user_interest_domains;

    let scored: Vec<FocusNode> = db
        .with_conn(move |conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT id, title, domain, quality_score, access_count, \
                            created_at, accessed_at \
                     FROM nodes \
                     WHERE ( \
                         created_at >= DATETIME('now', '-30 days') \
                         OR quality_score >= 0.7 \
                         OR access_count >= 3 \
                     ) \
                     ORDER BY created_at DESC \
                     LIMIT 2000",
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;

            let now_ts = chrono::Utc::now().timestamp() as f32;

            let rows = stmt
                .query_map([], |row| {
                    let id: String = row.get(0)?;
                    let title: String = row.get(1)?;
                    let domain: String = row.get(2)?;
                    let quality: f32 = row.get(3)?;
                    let access_count: u32 = row.get(4)?;
                    let created_at: String = row.get(5)?;
                    let accessed_at: String = row.get(6)?;
                    Ok((id, title, domain, quality, access_count, created_at, accessed_at))
                })
                .map_err(|e| BrainError::Database(e.to_string()))?;

            let mut focus_nodes = Vec::new();
            for row in rows {
                let (id, title, domain, quality, access_count, created_at, accessed_at) =
                    match row {
                        Ok(r) => r,
                        Err(_) => continue,
                    };

                // --- Compute sub-scores ---

                // Recency: how recently was this node accessed/created? (0..1)
                let accessed_ts = chrono::DateTime::parse_from_rfc3339(&accessed_at)
                    .map(|dt| dt.timestamp() as f32)
                    .unwrap_or(0.0);
                let created_ts = chrono::DateTime::parse_from_rfc3339(&created_at)
                    .map(|dt| dt.timestamp() as f32)
                    .unwrap_or(0.0);
                let most_recent = accessed_ts.max(created_ts);
                let age_hours = ((now_ts - most_recent) / 3600.0).max(0.0);
                let recency = 1.0 / (1.0 + age_hours / 24.0); // decay over days

                // Quality score (already 0..1)
                let quality_norm = quality;

                // Access frequency (normalized by max)
                let access_freq = (access_count as f32) / max_access;

                // Relevance to current project (binary match on domain)
                let relevance = if domain == project_clone { 1.0f32 } else { 0.0 };

                // Connection to active domains
                let domain_connection = if active_clone.contains(&domain) {
                    1.0f32
                } else {
                    0.0
                };

                // User interest alignment
                let user_interest = if interest_clone.contains(&domain) {
                    1.0f32
                } else {
                    0.0
                };

                // --- Weighted sum ---
                let attention_score = relevance * 0.35
                    + recency * 0.20
                    + quality_norm * 0.15
                    + access_freq * 0.10
                    + domain_connection * 0.10
                    + user_interest * 0.10;

                // Build reason string
                let mut reasons = Vec::new();
                if relevance > 0.5 {
                    reasons.push("current project");
                }
                if recency > 0.5 {
                    reasons.push("recent");
                }
                if quality_norm > 0.7 {
                    reasons.push("high quality");
                }
                if access_freq > 0.3 {
                    reasons.push("frequently accessed");
                }
                if domain_connection > 0.5 {
                    reasons.push("active domain");
                }
                if user_interest > 0.5 {
                    reasons.push("user interest");
                }
                let reason = if reasons.is_empty() {
                    "baseline".to_string()
                } else {
                    reasons.join(", ")
                };

                focus_nodes.push(FocusNode {
                    id,
                    title,
                    attention_score,
                    reason,
                });
            }

            // Sort descending by attention score, take top 100
            focus_nodes.sort_by(|a, b| {
                b.attention_score
                    .partial_cmp(&a.attention_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            focus_nodes.truncate(100);

            Ok(focus_nodes)
        })
        .await?;

    // Persist top 100 to attention_focus table
    let scored_clone = scored.clone();
    let now_clone = now.clone();
    db.with_conn(move |conn| {
        // Clear old focus
        conn.execute("DELETE FROM attention_focus", [])
            .map_err(|e| BrainError::Database(e.to_string()))?;

        for node in &scored_clone {
            conn.execute(
                "INSERT INTO attention_focus (node_id, attention_score, reason, updated_at) \
                 VALUES (?1, ?2, ?3, ?4)",
                params![node.id, node.attention_score, node.reason, now_clone],
            )
            .map_err(|e| BrainError::Database(e.to_string()))?;
        }
        Ok(())
    })
    .await?;

    let window = AttentionWindow {
        focus_nodes: scored,
        current_project,
        active_domains,
        last_updated: now,
    };

    log::info!(
        "Attention computed: {} focus nodes, project={}, domains={:?}",
        window.focus_nodes.len(),
        window.current_project,
        window.active_domains
    );

    Ok(window)
}

// =========================================================================
// GET FOCUS WINDOW
// =========================================================================

/// Return the top 100 nodes by attention score from the persisted table.
pub async fn get_focus_window(db: &Arc<BrainDb>) -> Result<AttentionWindow, BrainError> {
    let focus_nodes: Vec<FocusNode> = db
        .with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT af.node_id, n.title, af.attention_score, af.reason \
                     FROM attention_focus af \
                     LEFT JOIN nodes n ON n.id = af.node_id \
                     ORDER BY af.attention_score DESC \
                     LIMIT 100",
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt
                .query_map([], |row| {
                    Ok(FocusNode {
                        id: row.get(0)?,
                        title: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                        attention_score: row.get(2)?,
                        reason: row.get(3)?,
                    })
                })
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let mut results = Vec::new();
            for row in rows {
                if let Ok(r) = row {
                    results.push(r);
                }
            }
            Ok(results)
        })
        .await?;

    // Current project + active domains from recent data
    let current_project: String = db
        .with_conn(|conn| {
            let p: String = conn
                .query_row(
                    "SELECT domain FROM nodes \
                     WHERE created_at >= DATETIME('now', '-1 day') AND domain != '' \
                     GROUP BY domain ORDER BY COUNT(*) DESC LIMIT 1",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or_else(|_| "general".to_string());
            Ok(p)
        })
        .await?;

    let active_domains: Vec<String> = db
        .with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT domain FROM nodes \
                     WHERE created_at >= DATETIME('now', '-7 days') AND domain != '' \
                     GROUP BY domain ORDER BY COUNT(*) DESC LIMIT 5",
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let mut results = Vec::new();
            for row in rows {
                if let Ok(r) = row {
                    results.push(r);
                }
            }
            Ok(results)
        })
        .await?;

    let last_updated: String = db
        .with_conn(|conn| {
            let ts: String = conn
                .query_row(
                    "SELECT COALESCE(MAX(updated_at), '') FROM attention_focus",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or_default();
            Ok(ts)
        })
        .await?;

    Ok(AttentionWindow {
        focus_nodes,
        current_project,
        active_domains,
        last_updated,
    })
}

// =========================================================================
// UPDATE FOCUS
// =========================================================================

/// Shift attention when the user changes projects. Recomputes attention
/// with the new project context.
#[allow(dead_code)]
pub async fn update_focus(db: &Arc<BrainDb>, _project: &str) -> Result<AttentionWindow, BrainError> {
    // Recompute attention — the project detection is automatic from recent nodes
    compute_attention(db).await
}
