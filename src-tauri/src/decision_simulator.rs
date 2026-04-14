//! Phase Omega Part I — Decision Simulator
//!
//! Simulates how the user would decide on a given question/scenario by:
//! 1. Loading the cognitive fingerprint
//! 2. Searching the brain for relevant past decisions, cognition rules, and domain knowledge
//! 3. Building a rich context prompt
//! 4. Using the DEEP LLM tier to generate and score alternatives
//!
//! The result includes a recommended choice, reasoning, confidence, supporting
//! evidence from the brain, and alternative options scored against the
//! user's fingerprint.

use crate::cognitive_fingerprint::get_fingerprint;
use crate::db::BrainDb;
use crate::error::BrainError;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// =========================================================================
// DATA STRUCTURES
// =========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulatedDecision {
    pub question: String,
    pub recommended_choice: String,
    pub reasoning: String,
    pub confidence: f32,
    pub evidence: Vec<String>,
    pub fingerprint_alignment: f32,
    pub alternatives: Vec<Alternative>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alternative {
    pub choice: String,
    pub reasoning: String,
    pub score: f32,
}

// =========================================================================
// CORE SIMULATION
// =========================================================================

/// Simulate how the user would decide on the given question. Loads the
/// cognitive fingerprint, searches the brain for relevant context, and uses
/// the DEEP LLM to generate a structured decision.
pub async fn simulate_decision(
    db: &Arc<BrainDb>,
    question: &str,
) -> Result<SimulatedDecision, BrainError> {
    // 1. Load cognitive fingerprint
    let fingerprint = get_fingerprint(db)
        .await?
        .unwrap_or_default();

    // 2. Search brain for relevant knowledge (FTS5)
    let fts_results = db.search_nodes(question).await.unwrap_or_default();

    // 3. Try vector search as well (if embeddings are available)
    let emb_client = crate::embeddings::OllamaClient::new(
        db.config.ollama_url.clone(),
        db.config.embedding_model.clone(),
    );
    let mut vector_results = Vec::new();
    if emb_client.health_check().await {
        if let Ok(emb) = emb_client.generate_embedding(question).await {
            vector_results = db.vector_search(emb, 10).await.unwrap_or_default();
        }
    }

    // 4. Load relevant user cognition rules
    let question_owned = question.to_string();
    let cognition_rules: Vec<(String, String, f32)> = db
        .with_conn(move |conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT pattern_type, extracted_rule, confidence \
                     FROM user_cognition \
                     WHERE confidence > 0.4 \
                     ORDER BY confidence DESC \
                     LIMIT 30",
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
            let _ = question_owned; // consumed by move
            Ok(results)
        })
        .await?;

    // 5. Load past decisions from brain
    let decision_nodes: Vec<(String, String)> = db
        .with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT title, summary FROM nodes \
                     WHERE cognitive_type = 'decision' OR node_type = 'decision' \
                     ORDER BY created_at DESC \
                     LIMIT 20",
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let mut results = Vec::new();
            for row in rows {
                results.push(row.map_err(|e| BrainError::Database(e.to_string()))?);
            }
            Ok(results)
        })
        .await?;

    // 6. Build evidence list (titles of supporting nodes)
    let mut evidence: Vec<String> = Vec::new();
    for r in fts_results.iter().take(5) {
        evidence.push(r.node.title.clone());
    }
    for r in vector_results.iter().take(5) {
        if !evidence.contains(&r.node.title) {
            evidence.push(r.node.title.clone());
        }
    }

    // 7. Build context sections for the prompt
    let mut context = String::new();

    // Fingerprint summary
    context.push_str("## USER'S COGNITIVE FINGERPRINT\n");
    context.push_str(&format!(
        "- Risk tolerance: {:.2} | Decision speed: {:.2} | Info threshold: {:.2}\n",
        fingerprint.risk_tolerance, fingerprint.decision_speed, fingerprint.information_threshold
    ));
    context.push_str(&format!(
        "- Abstraction level: {:.2} | Directness: {:.2} | Technical depth: {:.2}\n",
        fingerprint.abstraction_level, fingerprint.directness, fingerprint.technical_depth
    ));
    context.push_str(&format!(
        "- Debugging style: {} | Iteration speed: {:.2}\n",
        fingerprint.debugging_style, fingerprint.iteration_speed
    ));
    if !fingerprint.approach_style.is_empty() {
        let styles: Vec<String> = fingerprint
            .approach_style
            .iter()
            .map(|(s, w)| format!("{}({:.2})", s, w))
            .collect();
        context.push_str(&format!("- Approach: {}\n", styles.join(", ")));
    }
    context.push('\n');

    // Relevant knowledge
    if !fts_results.is_empty() || !vector_results.is_empty() {
        context.push_str("## RELEVANT BRAIN KNOWLEDGE\n");
        for r in fts_results.iter().take(5) {
            let summary = crate::truncate_str(&r.node.summary, 200);
            context.push_str(&format!("- [{}] {}: {}\n", r.node.domain, r.node.title, summary));
        }
        for r in vector_results.iter().take(3) {
            if !fts_results.iter().any(|f| f.node.id == r.node.id) {
                let summary = crate::truncate_str(&r.node.summary, 200);
                context.push_str(&format!(
                    "- [{}] {}: {}\n",
                    r.node.domain, r.node.title, summary
                ));
            }
        }
        context.push('\n');
    }

    // User cognition rules
    if !cognition_rules.is_empty() {
        context.push_str("## USER'S ESTABLISHED PREFERENCES\n");
        for (pt, rule, conf) in cognition_rules.iter().take(10) {
            context.push_str(&format!("- [{}|{:.2}] {}\n", pt, conf, rule));
        }
        context.push('\n');
    }

    // Past decisions
    if !decision_nodes.is_empty() {
        context.push_str("## PAST DECISIONS\n");
        for (title, summary) in decision_nodes.iter().take(5) {
            let summary_trunc = crate::truncate_str(summary, 150);
            context.push_str(&format!("- {}: {}\n", title, summary_trunc));
        }
        context.push('\n');
    }

    // 8. Build the decision simulation prompt
    let prompt = format!(
        "You are simulating how a specific user would decide on a question. \
         Use their cognitive fingerprint, established preferences, past decisions, \
         and relevant knowledge to predict their decision.\n\n\
         {}\n\
         ## QUESTION\n{}\n\n\
         Output a JSON object (no markdown fences) with:\n\
         {{\n\
           \"recommended_choice\": \"<the choice this user would most likely make>\",\n\
           \"reasoning\": \"<why this user would choose this, referencing their patterns>\",\n\
           \"confidence\": <0.0-1.0>,\n\
           \"fingerprint_alignment\": <0.0-1.0, how well the recommendation aligns with their profile>,\n\
           \"alternatives\": [\n\
             {{ \"choice\": \"<alt 1>\", \"reasoning\": \"<why>\", \"score\": <0.0-1.0> }},\n\
             {{ \"choice\": \"<alt 2>\", \"reasoning\": \"<why>\", \"score\": <0.0-1.0> }}\n\
           ]\n\
         }}\n\
         Output ONLY the JSON object.",
        context, question
    );

    let llm = crate::commands::ai::get_llm_client_deep(db);
    let response = llm.generate(&prompt, 1000).await?;

    // 9. Parse the response
    let mut decision = SimulatedDecision {
        question: question.to_string(),
        recommended_choice: String::new(),
        reasoning: String::new(),
        confidence: 0.5,
        evidence,
        fingerprint_alignment: 0.5,
        alternatives: Vec::new(),
    };

    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(response.trim()) {
        if let Some(v) = parsed.get("recommended_choice").and_then(|v| v.as_str()) {
            decision.recommended_choice = v.to_string();
        }
        if let Some(v) = parsed.get("reasoning").and_then(|v| v.as_str()) {
            decision.reasoning = v.to_string();
        }
        if let Some(v) = parsed.get("confidence").and_then(|v| v.as_f64()) {
            decision.confidence = v as f32;
        }
        if let Some(v) = parsed.get("fingerprint_alignment").and_then(|v| v.as_f64()) {
            decision.fingerprint_alignment = v as f32;
        }
        if let Some(arr) = parsed.get("alternatives").and_then(|v| v.as_array()) {
            for item in arr {
                if let (Some(choice), Some(reasoning), Some(score)) = (
                    item.get("choice").and_then(|v| v.as_str()),
                    item.get("reasoning").and_then(|v| v.as_str()),
                    item.get("score").and_then(|v| v.as_f64()),
                ) {
                    decision.alternatives.push(Alternative {
                        choice: choice.to_string(),
                        reasoning: reasoning.to_string(),
                        score: score as f32,
                    });
                }
            }
        }
    } else {
        // Fallback: use raw LLM text as the recommendation
        log::warn!(
            "Decision simulator: failed to parse JSON, using raw text. Raw: {}",
            &response[..response.len().min(200)]
        );
        decision.recommended_choice = response.trim().to_string();
        decision.reasoning = "LLM response could not be parsed as structured JSON".into();
        decision.confidence = 0.3;
    }

    log::info!(
        "Decision simulated for '{}': '{}' (confidence={:.2})",
        crate::truncate_str(question, 60),
        crate::truncate_str(&decision.recommended_choice, 60),
        decision.confidence
    );

    Ok(decision)
}
