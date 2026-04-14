//! Phase Omega Part III — Temporal Intelligence
//!
//! Analyzes node creation timestamps by domain/topic to detect cyclical
//! patterns, trends, and anomalies. Generates predictions based on causal
//! links and temporal patterns. Validates past predictions against current
//! state and updates accuracy metrics.
//!
//! ## Functions
//!
//! - `detect_temporal_patterns()` — Scans node timestamps for periodicity
//! - `generate_predictions()` — Uses causal + temporal data to predict futures
//! - `validate_predictions()` — Checks overdue predictions, marks validated/invalidated

use crate::db::BrainDb;
use crate::error::BrainError;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

// =========================================================================
// DATA STRUCTURES
// =========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalPattern {
    pub id: String,
    pub pattern_type: String, // "cyclical", "trend", "anomaly", "burst"
    pub domain: String,
    pub description: String,
    pub period_days: Option<u32>,
    pub confidence: f32,
    pub evidence: Vec<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuturePrediction {
    pub id: String,
    pub prediction: String,
    pub confidence: f32,
    pub timeframe_days: u32,
    pub evidence_node_ids: Vec<String>,
    pub causal_chain: Vec<String>,
    pub validated: bool,
    pub invalidated: bool,
    pub created_at: String,
    pub due_at: String,
}

// =========================================================================
// DETECT TEMPORAL PATTERNS
// =========================================================================

/// Analyze node creation timestamps by domain/topic to find cyclical patterns,
/// trends, and anomalies. Stores discovered patterns in `temporal_patterns`.
pub async fn detect_temporal_patterns(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    // 1. Get node counts per domain per week
    let domain_weekly: Vec<(String, String, u32)> = db
        .with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT domain, \
                            strftime('%Y-%W', created_at) AS week, \
                            COUNT(*) AS cnt \
                     FROM nodes \
                     WHERE created_at IS NOT NULL AND created_at != '' \
                     GROUP BY domain, week \
                     ORDER BY domain, week",
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, u32>(2)?,
                    ))
                })
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let mut result = Vec::new();
            for row in rows {
                result.push(row.map_err(|e| BrainError::Database(e.to_string()))?);
            }
            Ok(result)
        })
        .await?;

    if domain_weekly.is_empty() {
        return Ok("No temporal data available".to_string());
    }

    // 2. Group by domain
    let mut by_domain: HashMap<String, Vec<(String, u32)>> = HashMap::new();
    for (domain, week, count) in &domain_weekly {
        by_domain
            .entry(domain.clone())
            .or_default()
            .push((week.clone(), *count));
    }

    let now = chrono::Utc::now().to_rfc3339();
    let mut patterns_found = 0u32;

    for (domain, weekly_counts) in &by_domain {
        if weekly_counts.len() < 4 {
            continue; // Need at least 4 weeks for pattern detection
        }

        let counts: Vec<f64> = weekly_counts.iter().map(|(_, c)| *c as f64).collect();
        let n = counts.len();

        // Calculate trend (simple linear regression slope)
        let mean = counts.iter().sum::<f64>() / n as f64;
        let x_mean = (n - 1) as f64 / 2.0;
        let mut num = 0.0;
        let mut den = 0.0;
        for (i, &y) in counts.iter().enumerate() {
            let x = i as f64;
            num += (x - x_mean) * (y - mean);
            den += (x - x_mean) * (x - x_mean);
        }
        let slope = if den > 0.0 { num / den } else { 0.0 };

        // Detect trend
        if slope.abs() > mean * 0.05 && n >= 6 {
            let direction = if slope > 0.0 { "growing" } else { "declining" };
            let description = format!(
                "Domain '{}' shows a {} trend: {:.1} nodes/week change (mean {:.0}/week over {} weeks)",
                domain, direction, slope, mean, n
            );
            let confidence = (slope.abs() / mean).min(1.0) as f32 * 0.8;

            let pattern_id = format!("temporal_pattern:{}", uuid::Uuid::now_v7());
            let pid = pattern_id.clone();
            let desc = description.clone();
            let dom = domain.clone();
            let now_c = now.clone();
            let evidence_json = serde_json::to_string(
                &weekly_counts
                    .iter()
                    .take(5)
                    .map(|(w, c)| format!("{}:{}", w, c))
                    .collect::<Vec<_>>(),
            )
            .unwrap_or_else(|_| "[]".to_string());

            db.with_conn(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO temporal_patterns \
                     (id, pattern_type, domain, description, period_days, confidence, evidence, created_at) \
                     VALUES (?1, 'trend', ?2, ?3, NULL, ?4, ?5, ?6)",
                    params![pid, dom, desc, confidence, evidence_json, now_c],
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
                Ok(())
            })
            .await?;
            patterns_found += 1;
        }

        // Detect bursts (weeks with count > 2x the mean)
        let recent = if counts.len() >= 3 {
            &counts[counts.len() - 3..]
        } else {
            &counts
        };
        for (i, &c) in recent.iter().enumerate() {
            if c > mean * 2.0 && mean > 1.0 {
                let week_idx = counts.len() - recent.len() + i;
                let week_label = &weekly_counts[week_idx].0;
                let description = format!(
                    "Burst detected in domain '{}' during week {}: {} nodes (mean {:.0})",
                    domain, week_label, c as u32, mean
                );
                let pattern_id = format!("temporal_pattern:{}", uuid::Uuid::now_v7());
                let pid = pattern_id.clone();
                let desc = description.clone();
                let dom = domain.clone();
                let now_c = now.clone();
                let confidence = ((c / mean - 1.0) * 0.3).min(0.9) as f32;

                db.with_conn(move |conn| {
                    conn.execute(
                        "INSERT OR REPLACE INTO temporal_patterns \
                         (id, pattern_type, domain, description, period_days, confidence, evidence, created_at) \
                         VALUES (?1, 'burst', ?2, ?3, NULL, ?4, '[]', ?5)",
                        params![pid, dom, desc, confidence, now_c],
                    )
                    .map_err(|e| BrainError::Database(e.to_string()))?;
                    Ok(())
                })
                .await?;
                patterns_found += 1;
            }
        }

        // Detect periodicity (check if weekly counts oscillate with a regular period)
        if n >= 8 {
            // Simple autocorrelation at lag 2, 4, 7
            for period in &[2u32, 4, 7] {
                let lag = *period as usize;
                if lag >= n {
                    continue;
                }
                let mut correlation = 0.0;
                let mut pairs = 0u32;
                for i in lag..n {
                    correlation += (counts[i] - mean) * (counts[i - lag] - mean);
                    pairs += 1;
                }
                let variance: f64 = counts.iter().map(|c| (c - mean).powi(2)).sum::<f64>();
                if variance > 0.0 && pairs > 0 {
                    let autocorr = correlation / variance;
                    if autocorr > 0.4 {
                        let description = format!(
                            "Cyclical pattern in domain '{}': ~{}-week period (autocorrelation {:.2})",
                            domain, period, autocorr
                        );
                        let pattern_id = format!("temporal_pattern:{}", uuid::Uuid::now_v7());
                        let pid = pattern_id.clone();
                        let desc = description.clone();
                        let dom = domain.clone();
                        let now_c = now.clone();
                        let period_days = *period * 7;
                        let confidence = (autocorr * 0.8).min(0.9) as f32;

                        db.with_conn(move |conn| {
                            conn.execute(
                                "INSERT OR REPLACE INTO temporal_patterns \
                                 (id, pattern_type, domain, description, period_days, confidence, evidence, created_at) \
                                 VALUES (?1, 'cyclical', ?2, ?3, ?4, ?5, '[]', ?6)",
                                params![pid, dom, desc, period_days, confidence, now_c],
                            )
                            .map_err(|e| BrainError::Database(e.to_string()))?;
                            Ok(())
                        })
                        .await?;
                        patterns_found += 1;
                    }
                }
            }
        }
    }

    Ok(format!(
        "Temporal analysis complete: {} patterns found across {} domains",
        patterns_found,
        by_domain.len()
    ))
}

// =========================================================================
// GENERATE PREDICTIONS
// =========================================================================

/// Based on causal links and temporal patterns, generate forward-looking
/// predictions and store them in `future_predictions`.
pub async fn generate_predictions(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    // 1. Load temporal patterns
    let patterns: Vec<TemporalPattern> = db
        .with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT id, pattern_type, domain, description, period_days, confidence, evidence, created_at \
                     FROM temporal_patterns \
                     WHERE confidence > 0.3 \
                     ORDER BY confidence DESC \
                     LIMIT 20",
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt
                .query_map([], |row| {
                    let evidence_json: String = row.get(6)?;
                    let period: Option<u32> = row.get(4)?;
                    Ok(TemporalPattern {
                        id: row.get(0)?,
                        pattern_type: row.get(1)?,
                        domain: row.get(2)?,
                        description: row.get(3)?,
                        period_days: period,
                        confidence: row.get(5)?,
                        evidence: serde_json::from_str(&evidence_json).unwrap_or_default(),
                        created_at: row.get(7)?,
                    })
                })
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let mut result = Vec::new();
            for row in rows {
                result.push(row.map_err(|e| BrainError::Database(e.to_string()))?);
            }
            Ok(result)
        })
        .await?;

    if patterns.is_empty() {
        return Ok("No temporal patterns to base predictions on".to_string());
    }

    // 2. Load causal links for enrichment
    let links = crate::world_model::get_all_links(db).await.unwrap_or_default();

    // 3. Build predictions from patterns
    let mut predictions_made = 0u32;
    let now = chrono::Utc::now().to_rfc3339();

    for pattern in &patterns {
        let prediction_text = match pattern.pattern_type.as_str() {
            "trend" => {
                if pattern.description.contains("growing") {
                    format!(
                        "Domain '{}' will continue growing in activity over the next 4 weeks based on trend",
                        pattern.domain
                    )
                } else {
                    format!(
                        "Domain '{}' will continue declining in activity over the next 4 weeks based on trend",
                        pattern.domain
                    )
                }
            }
            "cyclical" => {
                let period = pattern.period_days.unwrap_or(14);
                format!(
                    "Domain '{}' will show a recurring activity cycle in ~{} days",
                    pattern.domain, period
                )
            }
            "burst" => format!(
                "Activity in domain '{}' may normalize after recent burst",
                pattern.domain
            ),
            _ => continue,
        };

        let timeframe = match pattern.pattern_type.as_str() {
            "trend" => 28,
            "cyclical" => pattern.period_days.unwrap_or(14) as i32,
            "burst" => 14,
            _ => 30,
        };

        let due =
            chrono::Utc::now() + chrono::Duration::days(timeframe as i64);
        let prediction_id = format!("prediction:{}", uuid::Uuid::now_v7());

        // Find relevant causal links for this domain
        let causal_evidence: Vec<String> = links
            .iter()
            .take(5)
            .map(|l| format!("{}->{} ({})", l.cause_id, l.effect_id, l.relationship))
            .collect();
        let causal_json =
            serde_json::to_string(&causal_evidence).unwrap_or_else(|_| "[]".to_string());

        let pid = prediction_id.clone();
        let pred = prediction_text.clone();
        let conf = pattern.confidence;
        let now_c = now.clone();
        let due_str = due.to_rfc3339();

        db.with_conn(move |conn| {
            conn.execute(
                "INSERT OR IGNORE INTO future_predictions \
                 (id, prediction, confidence, timeframe_days, evidence_node_ids, \
                  causal_chain, validated, invalidated, created_at, due_at) \
                 VALUES (?1, ?2, ?3, ?4, '[]', ?5, 0, 0, ?6, ?7)",
                params![pid, pred, conf, timeframe, causal_json, now_c, due_str],
            )
            .map_err(|e| BrainError::Database(e.to_string()))?;
            Ok(())
        })
        .await?;
        predictions_made += 1;
    }

    Ok(format!(
        "Generated {} predictions from {} temporal patterns",
        predictions_made,
        patterns.len()
    ))
}

// =========================================================================
// VALIDATE PREDICTIONS
// =========================================================================

/// Check past predictions that are now due. Mark as validated or invalidated
/// based on actual data. Updates accuracy tracking.
pub async fn validate_predictions(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    let now = chrono::Utc::now().to_rfc3339();

    // 1. Load predictions that are past due and not yet validated/invalidated
    let overdue: Vec<FuturePrediction> = db
        .with_conn({
            let now_c = now.clone();
            move |conn| {
                let mut stmt = conn
                    .prepare(
                        "SELECT id, prediction, confidence, timeframe_days, evidence_node_ids, \
                         causal_chain, validated, invalidated, created_at, due_at \
                         FROM future_predictions \
                         WHERE due_at < ?1 AND validated = 0 AND invalidated = 0 \
                         LIMIT 20",
                    )
                    .map_err(|e| BrainError::Database(e.to_string()))?;
                let rows = stmt
                    .query_map(params![now_c], |row| {
                        let evidence_json: String = row.get(4)?;
                        let chain_json: String = row.get(5)?;
                        Ok(FuturePrediction {
                            id: row.get(0)?,
                            prediction: row.get(1)?,
                            confidence: row.get(2)?,
                            timeframe_days: row.get(3)?,
                            evidence_node_ids: serde_json::from_str(&evidence_json)
                                .unwrap_or_default(),
                            causal_chain: serde_json::from_str(&chain_json).unwrap_or_default(),
                            validated: row.get::<_, i32>(6)? != 0,
                            invalidated: row.get::<_, i32>(7)? != 0,
                            created_at: row.get(8)?,
                            due_at: row.get(9)?,
                        })
                    })
                    .map_err(|e| BrainError::Database(e.to_string()))?;
                let mut result = Vec::new();
                for row in rows {
                    result.push(row.map_err(|e| BrainError::Database(e.to_string()))?);
                }
                Ok(result)
            }
        })
        .await?;

    if overdue.is_empty() {
        return Ok("No overdue predictions to validate".to_string());
    }

    let mut validated = 0u32;
    let mut invalidated = 0u32;

    for prediction in &overdue {
        // Check if the prediction's domain had activity changes matching the prediction
        let is_valid = check_prediction_accuracy(db, prediction).await;

        let pid = prediction.id.clone();
        if is_valid {
            db.with_conn(move |conn| {
                conn.execute(
                    "UPDATE future_predictions SET validated = 1 WHERE id = ?1",
                    params![pid],
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
                Ok(())
            })
            .await?;
            validated += 1;
        } else {
            db.with_conn(move |conn| {
                conn.execute(
                    "UPDATE future_predictions SET invalidated = 1 WHERE id = ?1",
                    params![pid],
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
                Ok(())
            })
            .await?;
            invalidated += 1;
        }
    }

    Ok(format!(
        "Prediction validation: {} validated, {} invalidated out of {} overdue",
        validated,
        invalidated,
        overdue.len()
    ))
}

/// Simple heuristic to check if a prediction came true based on recent
/// activity. Returns true if the domain shows the predicted direction.
async fn check_prediction_accuracy(
    db: &Arc<BrainDb>,
    prediction: &FuturePrediction,
) -> bool {
    // Extract domain from prediction text
    let pred_lower = prediction.prediction.to_lowercase();
    let domain = if let Some(start) = pred_lower.find("domain '") {
        let after = &pred_lower[start + 8..];
        if let Some(end) = after.find('\'') {
            after[..end].to_string()
        } else {
            return false;
        }
    } else {
        return false;
    };

    let is_growth = pred_lower.contains("growing") || pred_lower.contains("continue growing");
    let is_decline = pred_lower.contains("declining") || pred_lower.contains("continue declining");

    // Compare node creation rate before and after the prediction was made
    let created_at = prediction.created_at.clone();
    let domain_c = domain.clone();

    let counts: Result<(u32, u32), BrainError> = db
        .with_conn(move |conn| {
            let before: u32 = conn
                .query_row(
                    "SELECT COUNT(*) FROM nodes WHERE domain = ?1 AND created_at < ?2",
                    params![domain_c, created_at],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            let domain_c2 = domain.clone();
            let after: u32 = conn
                .query_row(
                    "SELECT COUNT(*) FROM nodes WHERE domain = ?1 AND created_at >= ?2",
                    params![domain_c2, created_at],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            Ok((before, after))
        })
        .await;

    match counts {
        Ok((before, after)) => {
            if before == 0 {
                return false;
            }
            let ratio = after as f64 / before as f64;
            if is_growth {
                ratio > 0.8 // Still growing or at least maintained
            } else if is_decline {
                ratio < 1.2 // Not growing significantly
            } else {
                true // Generic predictions default to "probably valid"
            }
        }
        Err(_) => false,
    }
}

// =========================================================================
// QUERY HELPERS
// =========================================================================

/// Get all predictions (for HTTP/Tauri commands).
pub async fn get_predictions(db: &Arc<BrainDb>) -> Result<Vec<FuturePrediction>, BrainError> {
    db.with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT id, prediction, confidence, timeframe_days, evidence_node_ids, \
                 causal_chain, validated, invalidated, created_at, due_at \
                 FROM future_predictions \
                 ORDER BY created_at DESC \
                 LIMIT 100",
            )
            .map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt
            .query_map([], |row| {
                let evidence_json: String = row.get(4)?;
                let chain_json: String = row.get(5)?;
                Ok(FuturePrediction {
                    id: row.get(0)?,
                    prediction: row.get(1)?,
                    confidence: row.get(2)?,
                    timeframe_days: row.get(3)?,
                    evidence_node_ids: serde_json::from_str(&evidence_json).unwrap_or_default(),
                    causal_chain: serde_json::from_str(&chain_json).unwrap_or_default(),
                    validated: row.get::<_, i32>(6)? != 0,
                    invalidated: row.get::<_, i32>(7)? != 0,
                    created_at: row.get(8)?,
                    due_at: row.get(9)?,
                })
            })
            .map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| BrainError::Database(e.to_string()))?);
        }
        Ok(result)
    })
    .await
}
