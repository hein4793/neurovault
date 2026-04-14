//! Knowledge Compiler — Phase Omega Part IV
//!
//! Reads user_cognition + decision nodes and uses the LLM to extract
//! structured, machine-parseable rules. These compiled rules can then be
//! matched against any context to surface applicable knowledge instantly,
//! without needing a full LLM call.

use crate::db::BrainDb;
use crate::error::BrainError;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// =========================================================================
// Types
// =========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeRule {
    pub id: String,
    pub source_node_ids: Vec<String>,
    pub rule_type: String,      // "if_then", "always", "never", "prefer", "when_context"
    pub condition: String,      // machine-parseable condition
    pub action: String,         // what to do/recommend
    pub confidence: f32,
    pub times_applied: u32,
    pub times_correct: u32,
    pub accuracy: f32,
    pub compiled_at: String,
    pub invalidated: bool,
}

// =========================================================================
// compile_rules — extract structured rules from cognition + decisions
// =========================================================================

pub async fn compile_rules(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    // 1. Gather raw material: user cognition rules + decision nodes
    let cognition_rules: Vec<(String, String, String, f32)> = db
        .with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT id, pattern_type, extracted_rule, confidence
                     FROM user_cognition
                     WHERE confidence > 0.4
                     ORDER BY confidence DESC
                     LIMIT 50",
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, f32>(3)?,
                    ))
                })
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

    let decision_nodes: Vec<(String, String, String)> = db
        .with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT id, title, content FROM nodes
                     WHERE node_type = 'decision' OR cognitive_type = 'decision'
                     ORDER BY created_at DESC
                     LIMIT 30",
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
            let mut result = Vec::new();
            for r in rows {
                if let Ok(v) = r {
                    result.push(v);
                }
            }
            Ok(result)
        })
        .await?;

    if cognition_rules.is_empty() && decision_nodes.is_empty() {
        return Ok("No cognition or decision data to compile".to_string());
    }

    // 2. Build prompt for the LLM
    let mut context = String::new();
    context.push_str("USER COGNITION RULES:\n");
    for (id, ptype, rule, conf) in &cognition_rules {
        context.push_str(&format!(
            "- [{}] ({}, conf={:.2}): {}\n",
            id, ptype, conf, rule
        ));
    }
    context.push_str("\nDECISION NODES:\n");
    for (id, title, content) in &decision_nodes {
        let snippet = if content.len() > 300 {
            &content[..300]
        } else {
            content
        };
        context.push_str(&format!("- [{}] {}: {}\n", id, title, snippet));
    }

    let prompt = format!(
        "Analyze the following user cognition rules and decision records. \
         Extract structured, machine-parseable rules.\n\n\
         For each rule output EXACTLY one line in this format:\n\
         RULE|<rule_type>|<condition>|<action>|<source_ids comma-separated>\n\n\
         rule_type must be one of: if_then, always, never, prefer, when_context\n\
         condition: a short machine-readable condition (e.g. \"language=rust AND task=error_handling\")\n\
         action: what to do (e.g. \"use thiserror with BrainError enum\")\n\
         source_ids: IDs of the source cognition/decision records\n\n\
         Output 5-15 rules maximum. No preamble, no explanation.\n\n\
         {context}"
    );

    let llm = crate::commands::ai::get_llm_client_deep(db);
    let response = llm.generate(&prompt, 1200).await?;

    // 3. Parse LLM output into KnowledgeRule structs
    let now = chrono::Utc::now().to_rfc3339();
    let mut compiled = 0u32;

    for line in response.lines() {
        let line = line.trim();
        if !line.starts_with("RULE|") {
            continue;
        }
        let parts: Vec<&str> = line.splitn(5, '|').collect();
        if parts.len() < 5 {
            continue;
        }
        let rule_type = parts[1].trim().to_string();
        let condition = parts[2].trim().to_string();
        let action = parts[3].trim().to_string();
        let source_ids: Vec<String> = parts[4]
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        // Validate rule_type
        if !["if_then", "always", "never", "prefer", "when_context"].contains(&rule_type.as_str())
        {
            continue;
        }

        let rule_id = format!("kr:{}", uuid::Uuid::now_v7());
        let source_json =
            serde_json::to_string(&source_ids).unwrap_or_else(|_| "[]".to_string());
        let rid = rule_id.clone();
        let rt = rule_type.clone();
        let cond = condition.clone();
        let act = action.clone();
        let ts = now.clone();
        let sj = source_json.clone();

        let _ = db
            .with_conn(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO knowledge_rules
                     (id, source_node_ids, rule_type, condition, action,
                      confidence, times_applied, times_correct, accuracy,
                      compiled_at, invalidated)
                     VALUES (?1, ?2, ?3, ?4, ?5, 0.5, 0, 0, 0.0, ?6, 0)",
                    params![rid, sj, rt, cond, act, ts],
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
                Ok(())
            })
            .await;

        compiled += 1;
    }

    Ok(format!("Compiled {} rules from {} cognition + {} decision sources", compiled, cognition_rules.len(), decision_nodes.len()))
}

// =========================================================================
// apply_rule — match compiled rules against a context string
// =========================================================================

pub async fn apply_rule(db: &Arc<BrainDb>, context: &str) -> Result<Vec<KnowledgeRule>, BrainError> {
    let all_rules: Vec<KnowledgeRule> = db
        .with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT id, source_node_ids, rule_type, condition, action,
                            confidence, times_applied, times_correct, accuracy,
                            compiled_at, invalidated
                     FROM knowledge_rules
                     WHERE invalidated = 0
                     ORDER BY confidence DESC
                     LIMIT 200",
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt
                .query_map([], |row| {
                    Ok(KnowledgeRule {
                        id: row.get(0)?,
                        source_node_ids: serde_json::from_str(&row.get::<_, String>(1)?)
                            .unwrap_or_default(),
                        rule_type: row.get(2)?,
                        condition: row.get(3)?,
                        action: row.get(4)?,
                        confidence: row.get(5)?,
                        times_applied: row.get::<_, u32>(6)?,
                        times_correct: row.get::<_, u32>(7)?,
                        accuracy: row.get(8)?,
                        compiled_at: row.get(9)?,
                        invalidated: row.get::<_, i32>(10)? != 0,
                    })
                })
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

    let ctx_lower = context.to_lowercase();
    let ctx_words: std::collections::HashSet<String> = ctx_lower
        .split_whitespace()
        .filter(|w| w.len() > 3)
        .map(|s| s.trim_matches(|c: char| !c.is_alphanumeric()).to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let mut matching = Vec::new();
    for rule in all_rules {
        let cond_lower = rule.condition.to_lowercase();
        let cond_words: std::collections::HashSet<String> = cond_lower
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|w| w.len() > 2)
            .map(|s| s.to_string())
            .collect();

        // A rule matches if at least 2 condition words appear in the context
        let overlap = ctx_words.intersection(&cond_words).count();
        if overlap >= 2 {
            // Increment times_applied
            let rid = rule.id.clone();
            let _ = db
                .with_conn(move |conn| {
                    conn.execute(
                        "UPDATE knowledge_rules SET times_applied = times_applied + 1 WHERE id = ?1",
                        params![rid],
                    )
                    .map_err(|e| BrainError::Database(e.to_string()))?;
                    Ok(())
                })
                .await;
            matching.push(rule);
        }
    }

    Ok(matching)
}

// =========================================================================
// validate_rules — check rule accuracy, invalidate bad ones
// =========================================================================

pub async fn validate_rules(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    let rules: Vec<(String, u32, u32)> = db
        .with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT id, times_applied, times_correct
                     FROM knowledge_rules
                     WHERE invalidated = 0 AND times_applied > 0",
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, u32>(1)?,
                        row.get::<_, u32>(2)?,
                    ))
                })
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

    let mut invalidated = 0u32;
    let mut updated = 0u32;

    for (id, applied, correct) in &rules {
        let accuracy = if *applied > 0 {
            *correct as f32 / *applied as f32
        } else {
            0.0
        };

        // Invalidate rules with low accuracy after sufficient samples
        let should_invalidate = *applied >= 5 && accuracy < 0.3;

        let rid = id.clone();
        let inv = if should_invalidate { 1i32 } else { 0i32 };
        let acc = accuracy;
        let conf = (0.5 + accuracy * 0.5).min(1.0);

        let _ = db
            .with_conn(move |conn| {
                conn.execute(
                    "UPDATE knowledge_rules
                     SET accuracy = ?1, confidence = ?2, invalidated = ?3
                     WHERE id = ?4",
                    params![acc, conf, inv, rid],
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
                Ok(())
            })
            .await;

        if should_invalidate {
            invalidated += 1;
        } else {
            updated += 1;
        }
    }

    Ok(format!(
        "Validated {} rules: {} updated accuracy, {} invalidated",
        rules.len(),
        updated,
        invalidated
    ))
}
