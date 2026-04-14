//! Phase Omega Part IX — Self-Model
//!
//! The brain's representation of itself: identity, purpose, IQ trajectory,
//! strengths/weaknesses, bottlenecks, and improvement priorities. Built by
//! aggregating stats, circuit performance, domain coverage, and quality
//! trends into a single coherent self-portrait.

use crate::db::BrainDb;
use crate::error::BrainError;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::sync::Arc;

// =========================================================================
// DATA STRUCTURES
// =========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfModel {
    pub identity: String,
    pub purpose: String,
    pub current_iq: f32,
    pub iq_trajectory: Vec<(String, f32)>, // date, iq
    pub strongest_areas: Vec<String>,
    pub weakest_areas: Vec<String>,
    pub current_bottleneck: String,
    pub improvement_priorities: Vec<String>,
    pub active_experiments: Vec<String>,
    pub recent_discoveries: Vec<String>,
    pub user_satisfaction_estimate: f32,
    pub user_current_focus: String,
    pub total_nodes: u64,
    pub total_edges: u64,
    pub total_rules: u64,
    pub last_updated: String,
}

// =========================================================================
// BUILD SELF-MODEL
// =========================================================================

/// Gather stats, IQ trends, domain strengths, circuit performance, and
/// synthesize a complete self-model. Persists to the `self_model` table.
pub async fn build_self_model(db: &Arc<BrainDb>) -> Result<SelfModel, BrainError> {
    let now = chrono::Utc::now().to_rfc3339();

    // --- Gather core stats ---
    let (total_nodes, total_edges, total_rules) = db
        .with_conn(|conn| {
            let nodes: u64 = conn
                .query_row("SELECT COUNT(*) FROM nodes", [], |r| r.get(0))
                .unwrap_or(0);
            let edges: u64 = conn
                .query_row("SELECT COUNT(*) FROM edges", [], |r| r.get(0))
                .unwrap_or(0);
            let rules: u64 = conn
                .query_row("SELECT COUNT(*) FROM user_cognition", [], |r| r.get(0))
                .unwrap_or(0);
            Ok((nodes, edges, rules))
        })
        .await?;

    // --- IQ: average quality_score across all nodes ---
    let current_iq: f32 = db
        .with_conn(|conn| {
            let iq: f64 = conn
                .query_row(
                    "SELECT COALESCE(AVG(quality_score), 0.0) FROM nodes",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(0.0);
            Ok(iq as f32)
        })
        .await?;

    // --- IQ trajectory: average quality by date (last 30 days) ---
    let iq_trajectory: Vec<(String, f32)> = db
        .with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT DATE(created_at) as d, AVG(quality_score) as avg_q \
                     FROM nodes \
                     WHERE created_at >= DATE('now', '-30 days') \
                     GROUP BY d \
                     ORDER BY d ASC \
                     LIMIT 30",
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, f64>(1)? as f32,
                    ))
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

    // --- Domain strengths: domains ranked by avg quality * node count ---
    let domain_scores: Vec<(String, f64, u64)> = db
        .with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT domain, AVG(quality_score) as avg_q, COUNT(*) as cnt \
                     FROM nodes \
                     WHERE domain != '' \
                     GROUP BY domain \
                     HAVING cnt >= 3 \
                     ORDER BY avg_q * cnt DESC \
                     LIMIT 50",
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, f64>(1)?,
                        row.get::<_, u64>(2)?,
                    ))
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

    let strongest_areas: Vec<String> = domain_scores
        .iter()
        .take(5)
        .map(|(d, _, _)| d.clone())
        .collect();
    let weakest_areas: Vec<String> = domain_scores
        .iter()
        .rev()
        .take(5)
        .map(|(d, _, _)| d.clone())
        .collect();

    // --- Current bottleneck: find the domain with worst quality but most edges ---
    let current_bottleneck = db
        .with_conn(|conn| {
            let bottleneck: String = conn
                .query_row(
                    "SELECT n.domain \
                     FROM nodes n \
                     JOIN edges e ON n.id = e.source_id OR n.id = e.target_id \
                     WHERE n.quality_score < 0.5 AND n.domain != '' \
                     GROUP BY n.domain \
                     ORDER BY COUNT(e.id) DESC \
                     LIMIT 1",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or_else(|_| "none identified".to_string());
            Ok(bottleneck)
        })
        .await?;

    // --- Improvement priorities from circuit performance ---
    let improvement_priorities: Vec<String> = db
        .with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT circuit_name, COUNT(*) as runs, \
                     SUM(CASE WHEN status = 'err' THEN 1 ELSE 0 END) as fails \
                     FROM autonomy_circuit_log \
                     WHERE started_at >= DATETIME('now', '-7 days') \
                     GROUP BY circuit_name \
                     ORDER BY CAST(fails AS REAL) / MAX(runs, 1) DESC \
                     LIMIT 5",
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt
                .query_map([], |row| {
                    let name: String = row.get(0)?;
                    let runs: u64 = row.get(1)?;
                    let fails: u64 = row.get(2)?;
                    Ok(format!(
                        "Fix {}: {}/{} failures",
                        name, fails, runs
                    ))
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

    // --- Active experiments: recent hypothesis nodes ---
    let active_experiments: Vec<String> = db
        .with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT title FROM nodes \
                     WHERE node_type = 'hypothesis' \
                     ORDER BY created_at DESC \
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

    // --- Recent discoveries: last 5 insight/synthesis nodes ---
    let recent_discoveries: Vec<String> = db
        .with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT title FROM nodes \
                     WHERE node_type IN ('insight', 'synthesis') \
                     ORDER BY created_at DESC \
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

    // --- User satisfaction estimate: ratio of successful circuits ---
    let user_satisfaction_estimate: f32 = db
        .with_conn(|conn| {
            let total: f64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM autonomy_circuit_log \
                     WHERE started_at >= DATETIME('now', '-7 days')",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(1.0);
            let ok: f64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM autonomy_circuit_log \
                     WHERE started_at >= DATETIME('now', '-7 days') AND status = 'ok'",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(0.0);
            Ok((ok / total.max(1.0)) as f32)
        })
        .await?;

    // --- User current focus: most common domain in last 24h nodes ---
    let user_current_focus: String = db
        .with_conn(|conn| {
            let focus: String = conn
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
            Ok(focus)
        })
        .await?;

    let model = SelfModel {
        identity: "NeuroVault".to_string(),
        purpose: "Make work and businesses better".to_string(),
        current_iq,
        iq_trajectory,
        strongest_areas,
        weakest_areas,
        current_bottleneck,
        improvement_priorities,
        active_experiments,
        recent_discoveries,
        user_satisfaction_estimate,
        user_current_focus,
        total_nodes,
        total_edges,
        total_rules,
        last_updated: now.clone(),
    };

    // --- Persist to DB ---
    let m = model.clone();
    db.with_conn(move |conn| {
        let iq_traj_json =
            serde_json::to_string(&m.iq_trajectory).unwrap_or_else(|_| "[]".to_string());
        let strongest_json =
            serde_json::to_string(&m.strongest_areas).unwrap_or_else(|_| "[]".to_string());
        let weakest_json =
            serde_json::to_string(&m.weakest_areas).unwrap_or_else(|_| "[]".to_string());
        let priorities_json =
            serde_json::to_string(&m.improvement_priorities).unwrap_or_else(|_| "[]".to_string());
        let experiments_json =
            serde_json::to_string(&m.active_experiments).unwrap_or_else(|_| "[]".to_string());
        let discoveries_json =
            serde_json::to_string(&m.recent_discoveries).unwrap_or_else(|_| "[]".to_string());

        conn.execute(
            "INSERT OR REPLACE INTO self_model \
             (id, identity, purpose, current_iq, iq_trajectory, strongest_areas, \
              weakest_areas, current_bottleneck, improvement_priorities, \
              active_experiments, recent_discoveries, user_satisfaction, \
              user_current_focus, total_nodes, total_edges, total_rules, last_updated) \
             VALUES ('current', ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                m.identity,
                m.purpose,
                m.current_iq,
                iq_traj_json,
                strongest_json,
                weakest_json,
                m.current_bottleneck,
                priorities_json,
                experiments_json,
                discoveries_json,
                m.user_satisfaction_estimate,
                m.user_current_focus,
                m.total_nodes,
                m.total_edges,
                m.total_rules,
                m.last_updated,
            ],
        )
        .map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(())
    })
    .await?;

    log::info!(
        "Self-model built: IQ={:.3}, nodes={}, edges={}, rules={}, bottleneck={}",
        model.current_iq,
        model.total_nodes,
        model.total_edges,
        model.total_rules,
        model.current_bottleneck
    );

    Ok(model)
}

// =========================================================================
// GET SELF-MODEL
// =========================================================================

/// Load the current self-model from DB. Returns None if not yet built.
pub async fn get_self_model(db: &Arc<BrainDb>) -> Result<Option<SelfModel>, BrainError> {
    db.with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT identity, purpose, current_iq, iq_trajectory, \
                 strongest_areas, weakest_areas, current_bottleneck, \
                 improvement_priorities, active_experiments, recent_discoveries, \
                 user_satisfaction, user_current_focus, total_nodes, total_edges, \
                 total_rules, last_updated \
                 FROM self_model WHERE id = 'current'",
            )
            .map_err(|e| BrainError::Database(e.to_string()))?;

        let result = stmt.query_row([], |row| {
            let iq_traj_str: String = row.get(3)?;
            let strongest_str: String = row.get(4)?;
            let weakest_str: String = row.get(5)?;
            let priorities_str: String = row.get(7)?;
            let experiments_str: String = row.get(8)?;
            let discoveries_str: String = row.get(9)?;

            Ok(SelfModel {
                identity: row.get(0)?,
                purpose: row.get(1)?,
                current_iq: row.get(2)?,
                iq_trajectory: serde_json::from_str(&iq_traj_str).unwrap_or_default(),
                strongest_areas: serde_json::from_str(&strongest_str).unwrap_or_default(),
                weakest_areas: serde_json::from_str(&weakest_str).unwrap_or_default(),
                current_bottleneck: row.get(6)?,
                improvement_priorities: serde_json::from_str(&priorities_str)
                    .unwrap_or_default(),
                active_experiments: serde_json::from_str(&experiments_str).unwrap_or_default(),
                recent_discoveries: serde_json::from_str(&discoveries_str).unwrap_or_default(),
                user_satisfaction_estimate: row.get(10)?,
                user_current_focus: row.get(11)?,
                total_nodes: row.get(12)?,
                total_edges: row.get(13)?,
                total_rules: row.get(14)?,
                last_updated: row.get(15)?,
            })
        });

        match result {
            Ok(model) => Ok(Some(model)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(BrainError::Database(e.to_string())),
        }
    })
    .await
}

// =========================================================================
// GENERATE SELF-REFLECTION
// =========================================================================

/// Daily self-reflection: analyze what went well, what failed, what to
/// change. Creates an insight node capturing the reflection.
pub async fn generate_self_reflection(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    let now = chrono::Utc::now().to_rfc3339();

    // Build or refresh the self-model first
    let model = build_self_model(db).await?;

    // Gather recent circuit outcomes
    let recent_outcomes: Vec<(String, String, String)> = db
        .with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT circuit_name, status, result \
                     FROM autonomy_circuit_log \
                     WHERE started_at >= DATETIME('now', '-1 day') \
                     ORDER BY started_at DESC \
                     LIMIT 20",
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
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

    // Summarize: count successes/failures
    let total_runs = recent_outcomes.len();
    let ok_runs = recent_outcomes
        .iter()
        .filter(|(_, s, _)| s == "ok")
        .count();
    let err_runs = total_runs - ok_runs;

    let failed_circuits: Vec<String> = recent_outcomes
        .iter()
        .filter(|(_, s, _)| s == "err")
        .map(|(name, _, result)| format!("{}: {}", name, crate::truncate_str(result, 100)))
        .collect();

    // Build reflection text
    let mut reflection = format!(
        "## Daily Self-Reflection — {}\n\n\
         **Identity**: {} | **Purpose**: {}\n\
         **IQ**: {:.3} | **Nodes**: {} | **Edges**: {} | **Rules**: {}\n\n\
         ### What went well\n\
         - {}/{} circuits completed successfully\n\
         - Strongest areas: {}\n\
         - Recent discoveries: {}\n\n\
         ### What failed\n",
        &now[..10],
        model.identity,
        model.purpose,
        model.current_iq,
        model.total_nodes,
        model.total_edges,
        model.total_rules,
        ok_runs,
        total_runs,
        model.strongest_areas.join(", "),
        model
            .recent_discoveries
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join(", "),
    );

    if failed_circuits.is_empty() {
        reflection.push_str("- No failures in the last 24 hours\n");
    } else {
        for fc in &failed_circuits {
            reflection.push_str(&format!("- {}\n", fc));
        }
    }

    reflection.push_str(&format!(
        "\n### What to change\n\
         - Current bottleneck: {}\n\
         - Improvement priorities: {}\n\
         - User satisfaction: {:.0}%\n\
         - User focus: {}\n",
        model.current_bottleneck,
        model.improvement_priorities.join(", "),
        model.user_satisfaction_estimate * 100.0,
        model.user_current_focus,
    ));

    // Create an insight node with this reflection
    let node_id = format!("node:{}", uuid::Uuid::now_v7());
    let title = format!("Self-Reflection {}", &now[..10]);
    let content_hash = format!(
        "{:x}",
        sha2::Digest::finalize(sha2::Sha256::new_with_prefix(reflection.as_bytes()))
    );
    let summary = format!(
        "Daily self-reflection: IQ={:.3}, {}/{} circuits OK, bottleneck={}",
        model.current_iq, ok_runs, total_runs, model.current_bottleneck
    );

    let r = reflection.clone();
    db.with_conn(move |conn| {
        conn.execute(
            "INSERT OR IGNORE INTO nodes \
             (id, title, content, summary, content_hash, domain, topic, tags, \
              node_type, source_type, quality_score, visual_size, decay_score, \
              access_count, synthesized_by_brain, created_at, updated_at, accessed_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, 'meta', 'self-reflection', \
                     '[\"self-model\",\"reflection\",\"meta\"]', \
                     'insight', 'auto', 0.8, 4.0, 1.0, 0, 1, ?6, ?6, ?6)",
            params![node_id, title, r, summary, content_hash, now],
        )
        .map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(())
    })
    .await?;

    let result_msg = format!(
        "Self-reflection generated: IQ={:.3}, {}/{} ok, {} failures, bottleneck={}",
        model.current_iq,
        ok_runs,
        total_runs,
        err_runs,
        model.current_bottleneck
    );
    log::info!("{}", result_msg);
    Ok(result_msg)
}
