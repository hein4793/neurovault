//! Phase Omega Part I — Internal Dialogue System
//!
//! Runs a structured debate on any topic/question:
//! 1. Advocate argues FOR
//! 2. Critic argues AGAINST
//! 3. Synthesizer produces a verdict using the cognitive fingerprint
//!
//! Results are stored as brain nodes (`debate` + `decision`) linked with
//! `derived_from` edges, enriching the knowledge graph with reasoned
//! conclusions.

use crate::cognitive_fingerprint::get_fingerprint;
use crate::db::models::*;
use crate::db::BrainDb;
use crate::error::BrainError;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// =========================================================================
// DATA STRUCTURES
// =========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dialogue {
    pub topic: String,
    pub advocate_argument: String,
    pub critic_argument: String,
    pub synthesis: String,
    pub verdict: String,
    pub confidence: f32,
    pub created_at: String,
}

// =========================================================================
// CORE DIALOGUE ENGINE
// =========================================================================

/// Run a full advocate/critic/synthesizer dialogue on the given topic.
/// Creates `debate` and `decision` nodes in the brain, linked by
/// `derived_from` edges.
pub async fn run_dialogue(
    db: &Arc<BrainDb>,
    topic: &str,
) -> Result<Dialogue, BrainError> {
    let now = chrono::Utc::now().to_rfc3339();

    // 1. Load cognitive fingerprint and relevant context
    let fingerprint = get_fingerprint(db).await?.unwrap_or_default();

    // Search for relevant brain nodes
    let fts_results = db.search_nodes(topic).await.unwrap_or_default();

    // Load cognition rules
    let cognition_rules: Vec<(String, String, f32)> = db
        .with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT pattern_type, extracted_rule, confidence \
                     FROM user_cognition \
                     WHERE confidence > 0.4 \
                     ORDER BY confidence DESC \
                     LIMIT 20",
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

    // Build context string
    let mut context = String::new();
    if !fts_results.is_empty() {
        context.push_str("RELEVANT KNOWLEDGE:\n");
        for r in fts_results.iter().take(5) {
            let summary = crate::truncate_str(&r.node.summary, 200);
            context.push_str(&format!("- [{}] {}: {}\n", r.node.domain, r.node.title, summary));
        }
        context.push('\n');
    }
    if !cognition_rules.is_empty() {
        context.push_str("USER'S PREFERENCES:\n");
        for (pt, rule, conf) in cognition_rules.iter().take(8) {
            context.push_str(&format!("- [{}|{:.2}] {}\n", pt, conf, rule));
        }
        context.push('\n');
    }

    // 2. THREE separate LLM calls — Advocate, Critic, Synthesizer
    let llm = crate::commands::ai::get_llm_client_deep(db);

    // --- ADVOCATE ---
    let advocate_prompt = format!(
        "You are the ADVOCATE in an internal dialogue system. Your job is to argue \
         STRONGLY IN FAVOR of the following position/topic. Use the provided context \
         to support your argument. Be specific, cite evidence, and make the strongest \
         possible case.\n\n\
         {}\n\
         TOPIC: {}\n\n\
         Argue FOR this position in 3-5 concise paragraphs. Be persuasive and evidence-based.",
        context, topic
    );
    let advocate_argument = llm.generate(&advocate_prompt, 800).await?;

    // --- CRITIC ---
    let critic_prompt = format!(
        "You are the CRITIC in an internal dialogue system. Your job is to argue \
         STRONGLY AGAINST the following position/topic. Identify weaknesses, risks, \
         counterarguments, and alternative perspectives. Be specific and rigorous.\n\n\
         {}\n\
         TOPIC: {}\n\n\
         Argue AGAINST this position in 3-5 concise paragraphs. Be thorough and challenging.",
        context, topic
    );
    let critic_argument = llm.generate(&critic_prompt, 800).await?;

    // --- SYNTHESIZER ---
    let fingerprint_summary = format!(
        "Risk tolerance: {:.2}, Decision speed: {:.2}, Info threshold: {:.2}, \
         Directness: {:.2}, Technical depth: {:.2}, Debugging style: {}",
        fingerprint.risk_tolerance,
        fingerprint.decision_speed,
        fingerprint.information_threshold,
        fingerprint.directness,
        fingerprint.technical_depth,
        fingerprint.debugging_style,
    );

    let synth_prompt = format!(
        "You are the SYNTHESIZER in an internal dialogue system. Given the arguments \
         from both sides, and the user's cognitive fingerprint, produce a balanced \
         verdict that the USER would likely arrive at.\n\n\
         USER'S COGNITIVE PROFILE: {}\n\n\
         ADVOCATE'S ARGUMENT:\n{}\n\n\
         CRITIC'S ARGUMENT:\n{}\n\n\
         Output a JSON object (no markdown fences) with:\n\
         {{\n\
           \"synthesis\": \"<2-3 paragraphs synthesizing both arguments>\",\n\
           \"verdict\": \"<one-sentence clear verdict>\",\n\
           \"confidence\": <0.0-1.0>\n\
         }}\n\
         Output ONLY the JSON object.",
        fingerprint_summary, advocate_argument, critic_argument
    );
    let synth_response = llm.generate(&synth_prompt, 800).await?;

    // 3. Parse synthesizer response
    let mut synthesis = String::new();
    let mut verdict = String::new();
    let mut confidence: f32 = 0.5;

    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(synth_response.trim()) {
        if let Some(v) = parsed.get("synthesis").and_then(|v| v.as_str()) {
            synthesis = v.to_string();
        }
        if let Some(v) = parsed.get("verdict").and_then(|v| v.as_str()) {
            verdict = v.to_string();
        }
        if let Some(v) = parsed.get("confidence").and_then(|v| v.as_f64()) {
            confidence = v as f32;
        }
    } else {
        // Fallback: use the raw response as synthesis
        log::warn!(
            "Internal dialogue: failed to parse synthesizer JSON, using raw text"
        );
        synthesis = synth_response.trim().to_string();
        verdict = format!("See synthesis above (raw text, failed to parse structured output)");
        confidence = 0.3;
    }

    // 4. Create debate node
    let debate_content = format!(
        "## Topic\n{}\n\n## Advocate\n{}\n\n## Critic\n{}\n\n## Synthesis\n{}\n\n## Verdict\n{}",
        topic, advocate_argument, critic_argument, synthesis, verdict
    );
    let debate_node = db
        .create_node(CreateNodeInput {
            title: format!("Debate: {}", crate::truncate_str(topic, 80)),
            content: debate_content,
            domain: "cognition".to_string(),
            topic: "internal_dialogue".to_string(),
            tags: vec![
                "debate".into(),
                "internal_dialogue".into(),
                "phase_omega".into(),
            ],
            node_type: "synthesis".to_string(),
            source_type: "circuit".to_string(),
            source_url: None,
        })
        .await?;

    // Stamp cognitive markers on debate node
    let debate_id = debate_node.id.clone();
    let confidence_val = confidence;
    db.with_conn(move |conn| {
        conn.execute(
            "UPDATE nodes SET synthesized_by_brain = 1, cognitive_type = 'synthesis', \
             confidence = ?1 WHERE id = ?2",
            params![confidence_val, debate_id],
        )
        .map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(())
    })
    .await?;

    // 5. Create decision node with the verdict
    let decision_node = db
        .create_node(CreateNodeInput {
            title: format!("Decision: {}", crate::truncate_str(topic, 80)),
            content: format!("Verdict: {}\n\nConfidence: {:.2}\n\nBased on internal dialogue synthesis.", verdict, confidence),
            domain: "cognition".to_string(),
            topic: "internal_dialogue".to_string(),
            tags: vec![
                "decision".into(),
                "verdict".into(),
                "internal_dialogue".into(),
                "phase_omega".into(),
            ],
            node_type: NODE_TYPE_DECISION.to_string(),
            source_type: "circuit".to_string(),
            source_url: None,
        })
        .await?;

    // Stamp cognitive markers on decision node
    let decision_id = decision_node.id.clone();
    let confidence_val2 = confidence;
    db.with_conn(move |conn| {
        conn.execute(
            "UPDATE nodes SET synthesized_by_brain = 1, cognitive_type = 'decision', \
             confidence = ?1 WHERE id = ?2",
            params![confidence_val2, decision_id],
        )
        .map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(())
    })
    .await?;

    // 6. Create derived_from edges: decision -> debate, debate -> source nodes
    let edge_id_1 = format!("edge:{}", uuid::Uuid::now_v7());
    let now_c = now.clone();
    let dec_id = decision_node.id.clone();
    let deb_id = debate_node.id.clone();
    db.with_conn(move |conn| {
        conn.execute(
            "INSERT INTO edges (id, source_id, target_id, relation_type, strength, \
             discovered_by, evidence, animated, created_at, traversal_count) \
             VALUES (?1, ?2, ?3, 'derived_from', 0.95, 'circuit', \
             'Decision derived from internal dialogue', 1, ?4, 0)",
            params![edge_id_1, dec_id, deb_id, now_c],
        )
        .map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(())
    })
    .await?;

    // Link debate to source nodes it referenced
    for r in fts_results.iter().take(3) {
        let edge_id = format!("edge:{}", uuid::Uuid::now_v7());
        let now_c = now.clone();
        let src_id = debate_node.id.clone();
        let tgt_id = r.node.id.clone();
        let _ = db
            .with_conn(move |conn| {
                conn.execute(
                    "INSERT INTO edges (id, source_id, target_id, relation_type, strength, \
                     discovered_by, evidence, animated, created_at, traversal_count) \
                     VALUES (?1, ?2, ?3, 'derived_from', 0.8, 'circuit', \
                     'Source for internal dialogue', 0, ?4, 0)",
                    params![edge_id, src_id, tgt_id, now_c],
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
                Ok(())
            })
            .await;
    }

    log::info!(
        "Internal dialogue completed: '{}' -> verdict: '{}' (confidence={:.2})",
        crate::truncate_str(topic, 50),
        crate::truncate_str(&verdict, 80),
        confidence
    );

    Ok(Dialogue {
        topic: topic.to_string(),
        advocate_argument,
        critic_argument,
        synthesis,
        verdict,
        confidence,
        created_at: now,
    })
}

// =========================================================================
// CIRCUIT HELPER — Auto-pick a topic and run dialogue
// =========================================================================

/// Pick a recent unresolved question or controversial topic from the brain
/// and run an internal dialogue on it. Used by the circuit system.
pub async fn auto_dialogue(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    // Look for recent hypothesis or contradiction nodes that could benefit
    // from debate
    let topic: Option<(String, String)> = db
        .with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT id, title FROM nodes \
                     WHERE (cognitive_type = 'hypothesis' OR cognitive_type = 'contradiction' \
                            OR node_type = 'hypothesis' OR node_type = 'contradiction') \
                     AND id NOT IN (\
                         SELECT target_id FROM edges WHERE relation_type = 'derived_from' \
                         AND source_id IN (SELECT id FROM nodes WHERE topic = 'internal_dialogue')\
                     ) \
                     ORDER BY created_at DESC \
                     LIMIT 10",
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
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
        .await?
        .into_iter()
        // Use time-based offset to rotate through candidates
        .nth(
            (chrono::Utc::now().timestamp() as usize / 3600) % 10,
        );

    let (_node_id, title) = match topic {
        Some(t) => t,
        None => {
            // Fallback: pick a recent high-quality node
            let fallback: Option<(String, String)> = db
                .with_conn(|conn| {
                    let result = conn.query_row(
                        "SELECT id, title FROM nodes \
                         WHERE quality_score > 0.5 AND node_type != 'decision' \
                         ORDER BY RANDOM() LIMIT 1",
                        [],
                        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                    )
                    .ok();
                    Ok(result)
                })
                .await?;

            match fallback {
                Some(f) => f,
                None => return Ok("No suitable topics for internal dialogue".into()),
            }
        }
    };

    let dialogue = run_dialogue(db, &title).await?;

    Ok(format!(
        "Internal dialogue on '{}': verdict='{}' (confidence={:.2})",
        crate::truncate_str(&title, 50),
        crate::truncate_str(&dialogue.verdict, 80),
        dialogue.confidence
    ))
}
