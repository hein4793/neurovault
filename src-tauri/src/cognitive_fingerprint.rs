//! Phase Omega Part I — Cognitive Fingerprint
//!
//! Builds a structured profile of the user's cognitive patterns by analyzing
//! `user_cognition` entries and brain activity data. The fingerprint captures
//! decision-making style, problem-solving approach, communication preferences,
//! work patterns, and domain expertise — enabling the brain to predict how
//! the user would think about novel problems.

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
pub struct CognitiveFingerprint {
    // Decision-making
    pub risk_tolerance: f32,
    pub decision_speed: f32,
    pub information_threshold: f32,
    pub reversibility_preference: f32,

    // Problem-solving
    pub approach_style: Vec<(String, f32)>,
    pub abstraction_level: f32,
    pub iteration_speed: f32,
    pub debugging_style: String,

    // Communication
    pub verbosity: f32,
    pub formality: f32,
    pub directness: f32,
    pub technical_depth: f32,

    // Work patterns
    pub peak_hours: Vec<(u8, f32)>,
    pub context_switch_cost: f32,
    pub deep_work_duration: u32,

    // Domain expertise
    pub expertise: HashMap<String, ExpertiseProfile>,

    pub last_updated: String,
    pub version: u32,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpertiseProfile {
    pub depth: f32,
    pub breadth: f32,
    pub recency: f32,
    pub confidence: f32,
}

impl Default for CognitiveFingerprint {
    fn default() -> Self {
        Self {
            risk_tolerance: 0.5,
            decision_speed: 0.5,
            information_threshold: 0.5,
            reversibility_preference: 0.5,
            approach_style: vec![("analytical".into(), 0.5), ("intuitive".into(), 0.5)],
            abstraction_level: 0.5,
            iteration_speed: 0.5,
            debugging_style: "systematic".into(),
            verbosity: 0.5,
            formality: 0.5,
            directness: 0.5,
            technical_depth: 0.5,
            peak_hours: Vec::new(),
            context_switch_cost: 0.5,
            deep_work_duration: 90,
            expertise: HashMap::new(),
            last_updated: chrono::Utc::now().to_rfc3339(),
            version: 0,
            confidence: 0.0,
        }
    }
}

// =========================================================================
// DB OPERATIONS
// =========================================================================

/// Load the current fingerprint from the database. Returns None if not yet
/// synthesized.
pub async fn get_fingerprint(db: &Arc<BrainDb>) -> Result<Option<CognitiveFingerprint>, BrainError> {
    db.with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT risk_tolerance, decision_speed, information_threshold, \
                 reversibility_preference, approach_style, abstraction_level, \
                 iteration_speed, debugging_style, verbosity, formality, \
                 directness, technical_depth, peak_hours, context_switch_cost, \
                 deep_work_duration, expertise, last_updated, version, confidence \
                 FROM cognitive_fingerprint LIMIT 1",
            )
            .map_err(|e| BrainError::Database(e.to_string()))?;

        let fp = stmt
            .query_row([], |row| {
                Ok(CognitiveFingerprint {
                    risk_tolerance: row.get(0)?,
                    decision_speed: row.get(1)?,
                    information_threshold: row.get(2)?,
                    reversibility_preference: row.get(3)?,
                    approach_style: serde_json::from_str(
                        &row.get::<_, String>(4)?,
                    )
                    .unwrap_or_default(),
                    abstraction_level: row.get(5)?,
                    iteration_speed: row.get(6)?,
                    debugging_style: row.get(7)?,
                    verbosity: row.get(8)?,
                    formality: row.get(9)?,
                    directness: row.get(10)?,
                    technical_depth: row.get(11)?,
                    peak_hours: serde_json::from_str(
                        &row.get::<_, String>(12)?,
                    )
                    .unwrap_or_default(),
                    context_switch_cost: row.get(13)?,
                    deep_work_duration: row.get(14)?,
                    expertise: serde_json::from_str(
                        &row.get::<_, String>(15)?,
                    )
                    .unwrap_or_default(),
                    last_updated: row.get(16)?,
                    version: row.get(17)?,
                    confidence: row.get(18)?,
                })
            })
            .ok();

        Ok(fp)
    })
    .await
}

/// Store or update the fingerprint in the database.
async fn upsert_fingerprint(
    db: &Arc<BrainDb>,
    fp: &CognitiveFingerprint,
) -> Result<(), BrainError> {
    let fp = fp.clone();
    db.with_conn(move |conn| {
        let approach_json =
            serde_json::to_string(&fp.approach_style).unwrap_or_else(|_| "[]".into());
        let peak_json =
            serde_json::to_string(&fp.peak_hours).unwrap_or_else(|_| "[]".into());
        let expertise_json =
            serde_json::to_string(&fp.expertise).unwrap_or_else(|_| "{}".into());

        // Delete existing row (singleton table)
        conn.execute("DELETE FROM cognitive_fingerprint", [])
            .map_err(|e| BrainError::Database(e.to_string()))?;

        conn.execute(
            "INSERT INTO cognitive_fingerprint (\
             risk_tolerance, decision_speed, information_threshold, \
             reversibility_preference, approach_style, abstraction_level, \
             iteration_speed, debugging_style, verbosity, formality, \
             directness, technical_depth, peak_hours, context_switch_cost, \
             deep_work_duration, expertise, last_updated, version, confidence\
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19)",
            params![
                fp.risk_tolerance,
                fp.decision_speed,
                fp.information_threshold,
                fp.reversibility_preference,
                approach_json,
                fp.abstraction_level,
                fp.iteration_speed,
                fp.debugging_style,
                fp.verbosity,
                fp.formality,
                fp.directness,
                fp.technical_depth,
                peak_json,
                fp.context_switch_cost,
                fp.deep_work_duration,
                expertise_json,
                fp.last_updated,
                fp.version,
                fp.confidence,
            ],
        )
        .map_err(|e| BrainError::Database(e.to_string()))?;

        Ok(())
    })
    .await
}

// =========================================================================
// SYNTHESIS
// =========================================================================

/// Synthesize the cognitive fingerprint from user_cognition entries, activity
/// patterns, and domain statistics. Uses the DEEP LLM tier for analysis.
pub async fn synthesize_fingerprint(
    db: &Arc<BrainDb>,
) -> Result<CognitiveFingerprint, BrainError> {
    // 1. Load all user_cognition entries
    let cognition_rules: Vec<(String, String, f32)> = db
        .with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT pattern_type, extracted_rule, confidence \
                     FROM user_cognition \
                     WHERE confidence > 0.3 \
                     ORDER BY confidence DESC \
                     LIMIT 100",
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, f32>(2)?,
                    ))
                })
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let mut results = Vec::new();
            for row in rows {
                results.push(row.map_err(|e| BrainError::Database(e.to_string()))?);
            }
            Ok(results)
        })
        .await?;

    // 2. Read peak hours from node creation timestamps
    let hour_counts: Vec<(u8, u64)> = db
        .with_conn(|conn| {
            // Extract hour from ISO 8601 timestamps
            let mut stmt = conn
                .prepare(
                    "SELECT CAST(substr(created_at, 12, 2) AS INTEGER) AS hour, COUNT(*) AS cnt \
                     FROM nodes \
                     GROUP BY hour \
                     ORDER BY hour",
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt
                .query_map([], |row| Ok((row.get::<_, u8>(0)?, row.get::<_, u64>(1)?)))
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

    // 3. Read domain counts for expertise profiling
    let domain_stats: Vec<(String, u64, String, String)> = db
        .with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT domain, COUNT(*) AS cnt, \
                     MIN(created_at) AS oldest, MAX(created_at) AS newest \
                     FROM nodes \
                     WHERE domain != '' AND domain != 'general' \
                     GROUP BY domain \
                     ORDER BY cnt DESC \
                     LIMIT 50",
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, u64>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                })
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let mut results = Vec::new();
            for row in rows {
                results.push(row.map_err(|e| BrainError::Database(e.to_string()))?);
            }
            Ok(results)
        })
        .await?;

    // 4. Get the existing fingerprint version (if any)
    let existing = get_fingerprint(db).await?.unwrap_or_default();
    let new_version = existing.version + 1;

    // 5. Build prompt for LLM analysis
    let mut rules_text = String::new();
    for (pt, rule, conf) in &cognition_rules {
        rules_text.push_str(&format!("[{} | conf={:.2}] {}\n", pt, conf, rule));
    }
    if rules_text.is_empty() {
        rules_text.push_str("(no behavioral rules extracted yet)\n");
    }

    let mut hours_text = String::new();
    let total_nodes: u64 = hour_counts.iter().map(|(_, c)| c).sum();
    for (hour, count) in &hour_counts {
        let pct = if total_nodes > 0 {
            *count as f32 / total_nodes as f32
        } else {
            0.0
        };
        hours_text.push_str(&format!("  {}:00 — {} nodes ({:.1}%)\n", hour, count, pct * 100.0));
    }

    let mut domain_text = String::new();
    let max_count = domain_stats.first().map(|(_, c, _, _)| *c).unwrap_or(1);
    for (domain, count, oldest, newest) in &domain_stats {
        let depth = (*count as f32 / max_count as f32).min(1.0);
        domain_text.push_str(&format!(
            "  {} — {} nodes (depth={:.2}, from {} to {})\n",
            domain, count, depth, oldest, newest
        ));
    }

    let prompt = format!(
        "You are analyzing a user's cognitive patterns to build a structured profile.\n\n\
         ## Behavioral Rules Extracted From Past Sessions\n{}\n\n\
         ## Activity by Hour (UTC)\n{}\n\n\
         ## Domain Expertise (node counts)\n{}\n\n\
         Based on this data, output a JSON object (no markdown fences, no explanation) with \
         these exact fields, all values between 0.0 and 1.0 unless noted:\n\
         {{\n\
           \"risk_tolerance\": <0-1>,\n\
           \"decision_speed\": <0-1>,\n\
           \"information_threshold\": <0-1, how much info needed before deciding>,\n\
           \"reversibility_preference\": <0-1, preference for reversible choices>,\n\
           \"approach_style\": [[\"style_name\", weight], ...],\n\
           \"abstraction_level\": <0-1, 0=concrete, 1=abstract>,\n\
           \"iteration_speed\": <0-1>,\n\
           \"debugging_style\": \"<one of: systematic, intuitive, bisect, printf, hypothesis-driven>\",\n\
           \"verbosity\": <0-1>,\n\
           \"formality\": <0-1>,\n\
           \"directness\": <0-1>,\n\
           \"technical_depth\": <0-1>,\n\
           \"context_switch_cost\": <0-1, how much context-switching hurts productivity>,\n\
           \"deep_work_duration\": <minutes, typical deep work session length>,\n\
           \"confidence\": <0-1, how confident you are in this profile>\n\
         }}\n\
         Output ONLY the JSON object.",
        rules_text, hours_text, domain_text
    );

    let llm = crate::commands::ai::get_llm_client_deep(db);
    let response = llm.generate(&prompt, 800).await?;

    // 6. Parse LLM response
    let mut fp = existing.clone();
    fp.version = new_version;
    fp.last_updated = chrono::Utc::now().to_rfc3339();

    // Try to parse the JSON response, fallback to defaults on failure
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&response.trim()) {
        if let Some(v) = parsed.get("risk_tolerance").and_then(|v| v.as_f64()) {
            fp.risk_tolerance = v as f32;
        }
        if let Some(v) = parsed.get("decision_speed").and_then(|v| v.as_f64()) {
            fp.decision_speed = v as f32;
        }
        if let Some(v) = parsed.get("information_threshold").and_then(|v| v.as_f64()) {
            fp.information_threshold = v as f32;
        }
        if let Some(v) = parsed.get("reversibility_preference").and_then(|v| v.as_f64()) {
            fp.reversibility_preference = v as f32;
        }
        if let Some(arr) = parsed.get("approach_style").and_then(|v| v.as_array()) {
            let mut styles = Vec::new();
            for item in arr {
                if let Some(pair) = item.as_array() {
                    if pair.len() == 2 {
                        if let (Some(name), Some(weight)) =
                            (pair[0].as_str(), pair[1].as_f64())
                        {
                            styles.push((name.to_string(), weight as f32));
                        }
                    }
                }
            }
            if !styles.is_empty() {
                fp.approach_style = styles;
            }
        }
        if let Some(v) = parsed.get("abstraction_level").and_then(|v| v.as_f64()) {
            fp.abstraction_level = v as f32;
        }
        if let Some(v) = parsed.get("iteration_speed").and_then(|v| v.as_f64()) {
            fp.iteration_speed = v as f32;
        }
        if let Some(v) = parsed.get("debugging_style").and_then(|v| v.as_str()) {
            fp.debugging_style = v.to_string();
        }
        if let Some(v) = parsed.get("verbosity").and_then(|v| v.as_f64()) {
            fp.verbosity = v as f32;
        }
        if let Some(v) = parsed.get("formality").and_then(|v| v.as_f64()) {
            fp.formality = v as f32;
        }
        if let Some(v) = parsed.get("directness").and_then(|v| v.as_f64()) {
            fp.directness = v as f32;
        }
        if let Some(v) = parsed.get("technical_depth").and_then(|v| v.as_f64()) {
            fp.technical_depth = v as f32;
        }
        if let Some(v) = parsed.get("context_switch_cost").and_then(|v| v.as_f64()) {
            fp.context_switch_cost = v as f32;
        }
        if let Some(v) = parsed.get("deep_work_duration").and_then(|v| v.as_u64()) {
            fp.deep_work_duration = v as u32;
        }
        if let Some(v) = parsed.get("confidence").and_then(|v| v.as_f64()) {
            fp.confidence = v as f32;
        }
    } else {
        log::warn!(
            "Fingerprint synthesis: failed to parse LLM JSON, keeping previous values. Raw: {}",
            &response[..response.len().min(200)]
        );
        // Still bump version and timestamp even on parse failure
        fp.confidence = (fp.confidence * 0.9).max(0.1);
    }

    // 7. Build peak_hours from actual data
    let total_f = total_nodes.max(1) as f32;
    fp.peak_hours = hour_counts
        .iter()
        .map(|(h, c)| (*h, *c as f32 / total_f))
        .collect();

    // 8. Build expertise from domain stats
    let max_f = max_count.max(1) as f32;
    let now = chrono::Utc::now();
    fp.expertise.clear();
    for (domain, count, _oldest, newest) in &domain_stats {
        let depth = (*count as f32 / max_f).min(1.0);
        // Recency: days since last node in this domain
        let recency = chrono::DateTime::parse_from_rfc3339(newest)
            .map(|dt| {
                let days = (now - dt.with_timezone(&chrono::Utc))
                    .num_days()
                    .max(0) as f32;
                (1.0 - (days / 365.0).min(1.0)).max(0.0)
            })
            .unwrap_or(0.5);
        // Breadth: count distinct topics in this domain
        let breadth_val = depth.sqrt().min(1.0); // rough proxy
        fp.expertise.insert(
            domain.clone(),
            ExpertiseProfile {
                depth,
                breadth: breadth_val,
                recency,
                confidence: (depth * 0.6 + recency * 0.4).min(1.0),
            },
        );
    }

    // 9. Store to DB
    upsert_fingerprint(db, &fp).await?;

    log::info!(
        "Cognitive fingerprint synthesized (v{}, confidence={:.2}, {} domains)",
        fp.version,
        fp.confidence,
        fp.expertise.len()
    );

    Ok(fp)
}
