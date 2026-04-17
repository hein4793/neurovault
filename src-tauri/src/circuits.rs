//! Circuits — the rotating self-improvement engine.
//!
//! Phase 0 of the master plan: instead of running fixed-interval tasks
//! independently, the brain runs **one circuit every 20 minutes** from a
//! pool of seven self-improvement procedures, picked so the same circuit
//! never runs twice in the last three cycles. That gives 72 improvement
//! cycles per day instead of ~12, and crucially each circuit is small
//! enough to finish in ~5 minutes (compared to the old `quality_recalc`
//! that could hang for 12+ hours).
//!
//! Each circuit either creates new knowledge nodes ("thinking nodes" —
//! hypothesis, insight, decision, strategy, contradiction, prediction)
//! or strengthens the graph structure. Every cycle is logged to
//! `autonomy_circuit_log` and the rotation state is persisted to
//! `autonomy_circuit_rotation` so we recover correctly across restarts.
//!
//! ## The seven circuits
//!
//! | Slot | Circuit                  | Purpose                                                |
//! |------|--------------------------|--------------------------------------------------------|
//! | A    | meta_reflection          | Critique recent activity, generate research missions  |
//! | B    | user_pattern_mining      | Extract "how the user works" rules from chat history      |
//! | C    | cross_domain_fusion      | Aggressive cross-domain bridging (sim > 0.45)         |
//! | D    | quality_recalc           | Quality + decay rescore on a sampled batch            |
//! | E    | self_synthesis           | Generate insight/hypothesis nodes from clusters       |
//! | F    | curiosity_gap_fill       | Research a topic from the curiosity queue             |
//! | G    | iq_boost                 | Existing cross-domain bridges (sim > 0.6)             |

use crate::db::models::*;
use crate::db::BrainDb;
use crate::error::BrainError;
use rusqlite::params;
use std::sync::Arc;
use std::time::Instant;

/// All circuits in canonical order — Phase 0 (7) + Phase 1 (1) + Phase 2 (7) = 15 total.
///
/// Phase 2 circuits add the cognitive capabilities: contradiction detection,
/// decision extraction, knowledge synthesis, self-assessment, prediction
/// validation, hypothesis testing, and code-pattern extraction. Together
/// with the master cognitive loop they turn the brain from a memory store
/// into a self-evolving reasoning system.
pub const ALL_CIRCUITS: &[&str] = &[
    // Phase 0
    "meta_reflection",
    "user_pattern_mining",
    "cross_domain_fusion",
    "quality_recalc",
    "self_synthesis",
    "curiosity_gap_fill",
    "iq_boost",
    // Phase 1
    "compression_cycle",
    // Phase 2
    "contradiction_detector",
    "decision_memory_extractor",
    "knowledge_synthesizer",
    "self_assessment",
    "prediction_validator",
    "hypothesis_tester",
    "code_pattern_extractor",
    // Phase 4
    "synapse_prune",
    // Phase Omega
    "fingerprint_synthesis",
    "internal_dialogue",
    // Phase Omega II
    "swarm_orchestrator",
    // Phase Omega III
    "temporal_analysis",
    "causal_model_builder",
    "scenario_simulator",
    // Phase Omega IV — Recursive Self-Improvement
    "knowledge_compiler",
    "circuit_optimizer",
    "capability_tracker",
    // Phase Omega IX — Consciousness Layer
    "self_reflection",
    "attention_update",
    "curiosity_v2",
    // Phase Omega VI — Federation (The Collective)
    "federation_sync",
    // Phase Omega VII — Infrastructure
    "cluster_health_check",
    // Phase Omega VIII — Economic Autonomy
    "economic_audit",
    // Dual-Brain Phase 3 — Cross-Session Continuity
    "session_summarizer",
    // Dual-Brain Phase 4 — Context Quality
    "context_quality_optimizer",
    // Dual-Brain Phase 5 — Anticipatory Loading
    "anticipatory_preloader",
    // Dual-Brain Phase 6 — Dream Mode
    "deep_synthesis",
    "morning_briefing",
];

/// How many recent circuits to remember (and avoid repeating).
const ROTATION_WINDOW: usize = 3;

/// Result of a single circuit execution.
#[derive(Debug, Clone)]
pub struct CircuitResult {
    pub circuit_name: String,
    pub status: String, // "ok" | "err" | "skipped"
    pub result: String,
    pub duration_ms: u64,
}

// =========================================================================
// Public dispatch entry point
// =========================================================================

/// Pick the next circuit and run it. Called by the autonomy loop on its
/// 20-minute rotating schedule. Always logs to `autonomy_circuit_log` and
/// updates `autonomy_circuit_rotation`.
pub async fn dispatch_next_circuit(db: &Arc<BrainDb>) -> CircuitResult {
    let recent = load_recent_rotation(db).await;
    let circuit = pick_next_circuit(&recent);

    log::info!("Circuit dispatch → running '{}' (recent: {:?})", circuit, recent);

    let started_at = chrono::Utc::now().to_rfc3339();
    let start = Instant::now();

    let outcome = run_circuit(db, circuit).await;
    let duration_ms = start.elapsed().as_millis() as u64;

    let (status, result) = match outcome {
        Ok(msg) => ("ok".to_string(), msg),
        Err(e) => {
            log::warn!("Circuit '{}' failed: {}", circuit, e);
            ("err".to_string(), format!("FAILED: {}", e))
        }
    };

    // Log this run regardless of success/failure
    let _ = log_circuit_run(
        db,
        circuit,
        &started_at,
        duration_ms,
        &status,
        &result,
    )
    .await;

    // Update rotation state so the next pick avoids the last 3
    let mut new_recent = recent.clone();
    new_recent.insert(0, circuit.to_string());
    new_recent.truncate(ROTATION_WINDOW);
    let _ = save_rotation(db, &new_recent).await;

    log::info!(
        "Circuit '{}' done in {}ms ({}): {}",
        circuit, duration_ms, status, result
    );

    CircuitResult {
        circuit_name: circuit.to_string(),
        status,
        result,
        duration_ms,
    }
}

/// Pick the next circuit, avoiding any in `recent` (which holds at most
/// ROTATION_WINDOW entries, most-recent first).
///
/// Strategy: walk through ALL_CIRCUITS starting from the position right
/// after the most recent one, return the first that isn't in the recent
/// window. If all of them somehow are (only possible with a misconfigured
/// ROTATION_WINDOW), wrap around and pick the oldest one.
fn pick_next_circuit(recent: &[String]) -> &'static str {
    let last_idx = recent
        .first()
        .and_then(|name| ALL_CIRCUITS.iter().position(|c| c == name))
        .unwrap_or(ALL_CIRCUITS.len() - 1);

    // Walk forward from the slot after the most recent one
    for offset in 1..=ALL_CIRCUITS.len() {
        let idx = (last_idx + offset) % ALL_CIRCUITS.len();
        let candidate = ALL_CIRCUITS[idx];
        if !recent.iter().any(|r| r == candidate) {
            return candidate;
        }
    }

    // Fallback (shouldn't happen unless ROTATION_WINDOW >= ALL_CIRCUITS.len())
    ALL_CIRCUITS[(last_idx + 1) % ALL_CIRCUITS.len()]
}

// =========================================================================
// Circuit dispatch
// =========================================================================

/// Run the named circuit and return a human-readable result string.
async fn run_circuit(db: &Arc<BrainDb>, name: &str) -> Result<String, BrainError> {
    match name {
        // Phase 0
        "meta_reflection"     => circuit_meta_reflection(db).await,
        "user_pattern_mining" => circuit_user_pattern_mining(db).await,
        "cross_domain_fusion" => circuit_cross_domain_fusion(db).await,
        "quality_recalc"      => circuit_quality_recalc(db).await,
        "self_synthesis"      => circuit_self_synthesis(db).await,
        "curiosity_gap_fill"  => circuit_curiosity_gap_fill(db).await,
        "iq_boost"            => circuit_iq_boost(db).await,
        // Phase 1
        "compression_cycle"   => circuit_compression_cycle(db).await,
        // Phase 2
        "contradiction_detector"   => circuit_contradiction_detector(db).await,
        "decision_memory_extractor" => circuit_decision_memory_extractor(db).await,
        "knowledge_synthesizer"     => circuit_knowledge_synthesizer(db).await,
        "self_assessment"           => circuit_self_assessment(db).await,
        "prediction_validator"      => circuit_prediction_validator(db).await,
        "hypothesis_tester"         => circuit_hypothesis_tester(db).await,
        "code_pattern_extractor"    => circuit_code_pattern_extractor(db).await,
        // Phase 4
        "synapse_prune"             => circuit_synapse_prune(db).await,
        // Phase Omega
        "fingerprint_synthesis"     => circuit_fingerprint_synthesis(db).await,
        "internal_dialogue"         => circuit_internal_dialogue(db).await,
        // Phase Omega II
        "swarm_orchestrator"        => crate::swarm::circuit_swarm_orchestrator(db).await,
        // Phase Omega III
        "temporal_analysis"         => circuit_temporal_analysis(db).await,
        "causal_model_builder"      => circuit_causal_model_builder(db).await,
        "scenario_simulator"        => circuit_scenario_simulator(db).await,
        // Phase Omega IV — Recursive Self-Improvement
        "knowledge_compiler"        => circuit_knowledge_compiler(db).await,
        "circuit_optimizer"         => circuit_circuit_optimizer(db).await,
        "capability_tracker"        => circuit_capability_tracker(db).await,
        // Phase Omega IX — Consciousness Layer
        "self_reflection"           => circuit_self_reflection(db).await,
        "attention_update"          => circuit_attention_update(db).await,
        "curiosity_v2"              => circuit_curiosity_v2(db).await,
        // Phase Omega VI — Federation (The Collective)
        "federation_sync"           => crate::federation::circuit_federation_sync(db).await,
        // Phase Omega VII — Infrastructure
        "cluster_health_check"      => circuit_cluster_health_check(db).await,
        // Phase Omega VIII — Economic Autonomy
        "economic_audit"            => crate::economics::circuit_economic_audit(db).await,
        "session_summarizer"        => crate::session_continuity::circuit_session_summarizer(db).await,
        "context_quality_optimizer" => crate::context_quality::circuit_context_quality_optimizer(db).await,
        "anticipatory_preloader"    => crate::anticipatory::circuit_anticipatory_preloader(db).await,
        "deep_synthesis"            => crate::dream_mode::circuit_deep_synthesis(db).await,
        "morning_briefing"          => crate::dream_mode::circuit_morning_briefing(db).await,
        other => Err(BrainError::Internal(format!("Unknown circuit: {}", other))),
    }
}

// =========================================================================
// CIRCUIT D — quality_recalc (existing logic, sampled)
// =========================================================================

async fn circuit_quality_recalc(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    let (q, _) = crate::quality::scoring::calculate_quality_scores(db).await?;
    let (d, _) = crate::quality::decay::calculate_decay_scores(db).await?;
    Ok(format!("{} quality + {} decay scores updated", q, d))
}

// =========================================================================
// CIRCUIT G — iq_boost (existing cross-domain logic, threshold 0.6)
// =========================================================================

async fn circuit_iq_boost(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    let (created, _) = bridge_cross_domain(db, 0.6, 50).await?;
    Ok(format!("Created {} cross-domain edges (sim > 0.6)", created))
}

// =========================================================================
// CIRCUIT C — cross_domain_fusion (lower threshold + synthesis nodes)
// =========================================================================

async fn circuit_cross_domain_fusion(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    // Aggressive: lower the similarity threshold so we find more bridges
    let (edges_created, top_pairs) = bridge_cross_domain(db, 0.45, 100).await?;

    // For the strongest pairs, optionally generate a synthesis node that
    // captures the cross-domain insight. Limit to 3 per cycle so we don't
    // hammer Ollama.
    let mut synthesis_made = 0u32;
    if !top_pairs.is_empty() {
        // DEEP tier: cross-domain synthesis is high-stakes reasoning
        let llm = crate::commands::ai::get_llm_client_deep(db);
        for (id_a, id_b, sim) in top_pairs.iter().take(3) {
            if let Ok(node) = build_cross_domain_synthesis(db, &llm, id_a, id_b, *sim).await {
                synthesis_made += 1;
                log::info!("Cross-domain synthesis: {}", node);
            }
        }
    }

    Ok(format!(
        "Created {} fusion edges, {} synthesis insights",
        edges_created, synthesis_made
    ))
}

/// Shared cross-domain bridge builder. Returns (edges_created, top_pairs)
/// where top_pairs holds the strongest (source, target, sim) tuples for
/// optional follow-up synthesis.
async fn bridge_cross_domain(
    db: &Arc<BrainDb>,
    threshold: f64,
    cap: u64,
) -> Result<(u64, Vec<(String, String, f64)>), BrainError> {
    #[derive(Debug)]
    struct EmbNode {
        id: String,
        domain: String,
        embedding: Vec<f64>,
    }

    // Sample 5000 nodes with embeddings — full scan is too slow on 800K+
    let emb_nodes: Vec<EmbNode> = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, domain, embedding FROM nodes \
             WHERE embedding IS NOT NULL AND embedding != '' \
             LIMIT 5000"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            let emb_str: String = row.get(2)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                emb_str,
            ))
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut results = Vec::new();
        for row in rows {
            let (id, domain, emb_str) = row.map_err(|e| BrainError::Database(e.to_string()))?;
            if let Ok(embedding) = serde_json::from_str::<Vec<f64>>(&emb_str) {
                if !embedding.is_empty() {
                    results.push(EmbNode { id, domain, embedding });
                }
            }
        }
        Ok(results)
    }).await?;

    // Existing edges to dedupe
    let existing_edges: Vec<(String, String)> = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT source_id, target_id FROM edges LIMIT 100000"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| BrainError::Database(e.to_string()))?);
        }
        Ok(results)
    }).await?;

    let mut existing: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
    for (s, t) in &existing_edges {
        existing.insert((s.clone(), t.clone()));
        existing.insert((t.clone(), s.clone()));
    }

    let now = chrono::Utc::now().to_rfc3339();
    let mut by_domain: std::collections::HashMap<String, Vec<usize>> = std::collections::HashMap::new();
    for (i, node) in emb_nodes.iter().enumerate() {
        by_domain.entry(node.domain.clone()).or_default().push(i);
    }

    let mut created = 0u64;
    let mut top_pairs: Vec<(String, String, f64)> = Vec::new();
    let domain_keys: Vec<String> = by_domain.keys().cloned().collect();

    'outer: for i in 0..domain_keys.len() {
        for j in (i + 1)..domain_keys.len() {
            let nodes_a = &by_domain[&domain_keys[i]];
            let nodes_b = &by_domain[&domain_keys[j]];
            for &ai in nodes_a.iter().take(15) {
                for &bi in nodes_b.iter().take(15) {
                    let a = &emb_nodes[ai];
                    let b = &emb_nodes[bi];
                    let id_a = a.id.clone();
                    let id_b = b.id.clone();
                    if existing.contains(&(id_a.clone(), id_b.clone())) { continue; }

                    let sim = cosine_sim(&a.embedding, &b.embedding);
                    if sim > threshold {
                        let strength = ((sim - 0.4) * 1.5).min(0.95);
                        let evidence = format!("Circuit cross-domain: {:.0}% similarity", sim * 100.0);
                        let edge_id = format!("edges:{}", uuid::Uuid::now_v7());
                        let now_c = now.clone();
                        let id_a_c = id_a.clone();
                        let id_b_c = id_b.clone();
                        let _ = db.with_conn(move |conn| {
                            conn.execute(
                                "INSERT INTO edges (id, source_id, target_id, relation_type, strength, \
                                 discovered_by, evidence, animated, created_at, traversal_count) \
                                 VALUES (?1, ?2, ?3, 'cross_domain', ?4, 'circuit', ?5, 1, ?6, 0)",
                                params![edge_id, id_a_c, id_b_c, strength, evidence, now_c],
                            ).map_err(|e| BrainError::Database(e.to_string()))?;
                            Ok(())
                        }).await;

                        existing.insert((id_a.clone(), id_b.clone()));
                        if sim > 0.55 {
                            top_pairs.push((id_a, id_b, sim));
                        }
                        created += 1;
                        if created >= cap { break 'outer; }
                    }
                }
            }
        }
    }

    // Sort top pairs by similarity (desc) so synthesis picks strongest first
    top_pairs.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    top_pairs.truncate(10);
    Ok((created, top_pairs))
}

/// Build a synthesis "insight" node that captures the cross-domain link.
async fn build_cross_domain_synthesis(
    db: &Arc<BrainDb>,
    llm: &crate::ai::client::LlmClient,
    id_a: &str,
    id_b: &str,
    sim: f64,
) -> Result<String, BrainError> {
    // Fetch summaries of both endpoints
    #[derive(Debug)]
    struct NodeBrief {
        title: String,
        summary: String,
        domain: String,
    }

    let id_a_owned = id_a.to_string();
    let id_b_owned = id_b.to_string();

    let a = db.with_conn(move |conn| {
        conn.query_row(
            "SELECT title, summary, domain FROM nodes WHERE id = ?1",
            params![id_a_owned],
            |row| Ok(NodeBrief {
                title: row.get(0)?,
                summary: row.get(1)?,
                domain: row.get(2)?,
            }),
        ).map_err(|e| BrainError::Database(e.to_string()))
    }).await.ok();

    let b = db.with_conn(move |conn| {
        conn.query_row(
            "SELECT title, summary, domain FROM nodes WHERE id = ?1",
            params![id_b_owned],
            |row| Ok(NodeBrief {
                title: row.get(0)?,
                summary: row.get(1)?,
                domain: row.get(2)?,
            }),
        ).map_err(|e| BrainError::Database(e.to_string()))
    }).await.ok();

    let (a, b) = match (a, b) {
        (Some(a), Some(b)) => (a, b),
        _ => return Err(BrainError::Internal("missing endpoint".into())),
    };

    let prompt = format!(
        "Two knowledge nodes from different domains were linked by semantic similarity ({:.0}%):\n\n\
         Domain A: {}\nTitle: {}\nSummary: {}\n\n\
         Domain B: {}\nTitle: {}\nSummary: {}\n\n\
         In one sentence, what is the non-obvious connection or insight that links them? \
         Be concrete and specific. If there's no real insight, reply just \"NONE\".",
        sim * 100.0, a.domain, a.title, a.summary, b.domain, b.title, b.summary
    );
    let insight = llm.generate(&prompt, 200).await?;
    if insight.trim().to_uppercase().starts_with("NONE") || insight.trim().len() < 20 {
        return Err(BrainError::Internal("no real insight".into()));
    }

    // Create the synthesis insight node
    let title = format!("Insight: {} ↔ {}", a.title, b.title);
    let topic = format!("{}+{}", a.domain, b.domain);
    let input = CreateNodeInput {
        title,
        content: insight.clone(),
        domain: "synthesis".to_string(),
        topic,
        tags: vec!["insight".into(), "cross-domain".into(), "circuit".into()],
        node_type: NODE_TYPE_INSIGHT.to_string(),
        source_type: "synthesis".to_string(),
        source_url: None,
    };
    let created = db.create_node(input).await?;

    // Stamp the cognitive markers
    let created_id = created.id.clone();
    let confidence_val = (sim as f32 - 0.45).max(0.0).min(1.0);
    db.with_conn(move |conn| {
        conn.execute(
            "UPDATE nodes SET synthesized_by_brain = 1, cognitive_type = ?1, confidence = ?2 WHERE id = ?3",
            params![NODE_TYPE_INSIGHT, confidence_val, created_id],
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(())
    }).await?;

    // Create derived_from edges to both source nodes
    for src_id in [id_a, id_b] {
        let edge_id = format!("edges:{}", uuid::Uuid::now_v7());
        let now = chrono::Utc::now().to_rfc3339();
        let src_node_id = created.id.clone();
        let tgt_node_id = src_id.to_string();
        let _ = db.with_conn(move |conn| {
            conn.execute(
                "INSERT INTO edges (id, source_id, target_id, relation_type, strength, \
                 discovered_by, evidence, animated, created_at, traversal_count) \
                 VALUES (?1, ?2, ?3, 'derived_from', 0.9, 'circuit', 'Source for synthesis insight', 0, ?4, 0)",
                params![edge_id, src_node_id, tgt_node_id, now],
            ).map_err(|e| BrainError::Database(e.to_string()))?;
            Ok(())
        }).await;
    }

    Ok(created.id)
}

// =========================================================================
// CIRCUIT B — user_pattern_mining
// =========================================================================
//
// Scan recent chat_history nodes, ask the LLM to extract behavioral
// patterns (coding style, debug flow, planning, etc.), and store them in
// `user_cognition`. If the same rule already exists, increment its
// `times_confirmed` counter; if it conflicts, increment
// `times_contradicted` and decay confidence.

async fn circuit_user_pattern_mining(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    // Pull last 30 chat nodes (small window so each cycle is fast)
    #[derive(Debug)]
    struct ChatNode {
        id: String,
        content: String,
    }
    let chats: Vec<ChatNode> = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, content FROM nodes \
             WHERE source_type = 'chat_history' OR source_type = 'auto_sync' \
             ORDER BY created_at DESC LIMIT 30"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok(ChatNode {
                id: row.get(0)?,
                content: row.get(1)?,
            })
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| BrainError::Database(e.to_string()))?);
        }
        Ok(results)
    }).await?;

    if chats.is_empty() {
        return Ok("No chat history yet — pattern mining skipped".into());
    }

    // Build a single corpus from the chunk so we send one LLM call (not 30).
    // Cap content to ~6000 chars total to fit in the 8k context window.
    let mut corpus = String::new();
    let mut trigger_ids: Vec<String> = Vec::new();
    for chat in &chats {
        if corpus.len() > 6000 { break; }
        let take = chat.content.chars().take(800).collect::<String>();
        corpus.push_str(&take);
        corpus.push_str("\n---\n");
        trigger_ids.push(chat.id.clone());
    }

    // FAST tier: pattern extraction is high-frequency and tractable
    let llm = crate::commands::ai::get_llm_client_fast(db);
    let prompt = format!(
        "You are analysing recent conversations to extract concrete behavioral and preference \
         patterns about the user (the user). Output 1-5 specific, actionable rules in this exact format, \
         one per line, with no preamble or numbering:\n\n\
         PATTERN_TYPE | RULE\n\n\
         PATTERN_TYPE must be one of: coding_style | debug_flow | planning | refactoring | naming \
         | tooling | communication | decision_making | testing | error_handling.\n\
         RULE must be a single short sentence describing a specific repeatable preference. \
         Skip generic platitudes; only extract patterns clearly evidenced in the text.\n\
         If no real patterns are evident, output just NONE.\n\n\
         Conversations:\n{}",
        corpus
    );

    let response = llm.generate(&prompt, 600).await?;
    if response.trim().to_uppercase().contains("NONE") && response.trim().len() < 20 {
        return Ok("No clear patterns extracted this cycle".into());
    }

    let mut new_count = 0u32;
    let mut confirmed_count = 0u32;
    let now = chrono::Utc::now().to_rfc3339();

    for line in response.lines() {
        let line = line.trim();
        if line.is_empty() || !line.contains('|') { continue; }
        let parts: Vec<&str> = line.splitn(2, '|').collect();
        if parts.len() != 2 { continue; }
        let pattern_type = parts[0].trim().to_lowercase();
        let rule = parts[1].trim().to_string();
        if rule.len() < 10 || rule.len() > 300 { continue; }

        // Check if this rule already exists (fuzzy: same pattern_type + similar text)
        #[derive(Debug)]
        struct ExistingRule {
            id: String,
            extracted_rule: String,
            times_confirmed: u32,
        }
        let pt_clone = pattern_type.clone();
        let existing: Vec<ExistingRule> = db.with_conn(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, extracted_rule, times_confirmed FROM user_cognition \
                 WHERE pattern_type = ?1 LIMIT 50"
            ).map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt.query_map(params![pt_clone], |row| {
                Ok(ExistingRule {
                    id: row.get(0)?,
                    extracted_rule: row.get(1)?,
                    times_confirmed: row.get(2)?,
                })
            }).map_err(|e| BrainError::Database(e.to_string()))?;
            let mut results = Vec::new();
            for row in rows {
                results.push(row.map_err(|e| BrainError::Database(e.to_string()))?);
            }
            Ok(results)
        }).await?;

        let dup = existing.iter().find(|r| {
            // Cheap similarity: shared word ratio
            similar_text(&r.extracted_rule, &rule) > 0.55
        });

        if let Some(dup) = dup {
            // Confirm existing rule — bump times_confirmed, recompute confidence
            let rid = dup.id.clone();
            let new_confirmed = dup.times_confirmed + 1;
            let new_confidence = ((new_confirmed as f32) / (new_confirmed as f32 + 1.0))
                .min(0.99);
            let now_c = now.clone();
            let _ = db.with_conn(move |conn| {
                conn.execute(
                    "UPDATE user_cognition SET times_confirmed = ?1, confidence = ?2, timestamp = ?3 WHERE id = ?4",
                    params![new_confirmed, new_confidence, now_c, rid],
                ).map_err(|e| BrainError::Database(e.to_string()))?;
                Ok(())
            }).await;
            confirmed_count += 1;
        } else {
            // Insert brand new rule
            let cog_id = format!("user_cognition:{}", uuid::Uuid::now_v7());
            let trigger_json = serde_json::to_string(&trigger_ids).unwrap_or_else(|_| "[]".into());
            let linked_json = "[]".to_string();
            let now_c = now.clone();
            let pt_c = pattern_type.clone();
            let rule_c = rule.clone();
            let _ = db.with_conn(move |conn| {
                conn.execute(
                    "INSERT INTO user_cognition (id, timestamp, trigger_node_ids, pattern_type, \
                     extracted_rule, structured_rule, confidence, times_confirmed, times_contradicted, \
                     embedding, linked_to_nodes) \
                     VALUES (?1, ?2, ?3, ?4, ?5, NULL, 0.5, 1, 0, NULL, ?6)",
                    params![cog_id, now_c, trigger_json, pt_c, rule_c, linked_json],
                ).map_err(|e| BrainError::Database(e.to_string()))?;
                Ok(())
            }).await;
            new_count += 1;
        }
    }

    Ok(format!(
        "{} new + {} confirmed user_cognition rules from {} chats",
        new_count, confirmed_count, chats.len()
    ))
}

/// Cheap text similarity for deduping rules — Jaccard on lowercased
/// word sets. Not perfect but good enough for our use case.
/// Pub so master_loop and other modules can reuse it.
pub fn similar_text(a: &str, b: &str) -> f64 {
    let words_a: std::collections::HashSet<String> = a
        .to_lowercase()
        .split_whitespace()
        .filter(|w| w.len() > 3)
        .map(|s| s.to_string())
        .collect();
    let words_b: std::collections::HashSet<String> = b
        .to_lowercase()
        .split_whitespace()
        .filter(|w| w.len() > 3)
        .map(|s| s.to_string())
        .collect();
    let inter = words_a.intersection(&words_b).count();
    let union = words_a.union(&words_b).count();
    if union == 0 { 0.0 } else { inter as f64 / union as f64 }
}

// =========================================================================
// CIRCUIT E — self_synthesis
// =========================================================================
//
// Pick a topic cluster, load its top nodes, ask the LLM to extract a new
// insight or hypothesis the brain didn't already have, and store it as a
// thinking node with synthesized_by_brain=true and `derived_from` edges.

async fn circuit_self_synthesis(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    #[derive(Debug)]
    struct TopicGroup { topic: String, count: u64 }

    let topics: Vec<TopicGroup> = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT topic, COUNT(*) AS count FROM nodes WHERE topic != '' GROUP BY topic LIMIT 200"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok(TopicGroup {
                topic: row.get(0)?,
                count: row.get(1)?,
            })
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| BrainError::Database(e.to_string()))?);
        }
        Ok(results)
    }).await?;

    if topics.is_empty() {
        return Ok("No topics found — synthesis skipped".into());
    }

    // Use a time-based offset to rotate through topics across cycles
    let topic_idx = (chrono::Utc::now().timestamp() as usize / 1200) % topics.len();
    let target = &topics[topic_idx];
    if target.count < 3 {
        return Ok(format!("Topic '{}' too small ({} nodes) — synthesis skipped", target.topic, target.count));
    }

    #[derive(Debug)]
    struct ClusterNode {
        id: String,
        title: String,
        summary: String,
        domain: String,
    }
    let topic_clone = target.topic.clone();
    let cluster: Vec<ClusterNode> = db.with_conn(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, title, summary, domain FROM nodes WHERE topic = ?1 LIMIT 30"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map(params![topic_clone], |row| {
            Ok(ClusterNode {
                id: row.get(0)?,
                title: row.get(1)?,
                summary: row.get(2)?,
                domain: row.get(3)?,
            })
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| BrainError::Database(e.to_string()))?);
        }
        Ok(results)
    }).await?;

    if cluster.len() < 3 {
        return Ok(format!("Cluster '{}' too sparse for synthesis", target.topic));
    }

    // Build a brief corpus for the LLM
    let mut corpus = String::new();
    for n in cluster.iter().take(15) {
        corpus.push_str(&format!("- [{}] {}: {}\n", n.domain, n.title, n.summary));
    }

    // DEEP tier: insight generation needs the higher-quality model
    let llm = crate::commands::ai::get_llm_client_deep(db);
    let prompt = format!(
        "You are extracting one new insight or hypothesis from a cluster of related knowledge nodes. \
         Output exactly one line in the format:\n\n\
         TYPE | TITLE | CLAIM\n\n\
         TYPE must be one of: insight | hypothesis | strategy.\n\
         TITLE is a short (≤80 char) headline.\n\
         CLAIM is one to two sentences making a specific, falsifiable, non-trivial statement about the topic. \
         If nothing notable, output just NONE.\n\n\
         Topic: {}\nNodes:\n{}",
        target.topic, corpus
    );

    let response = llm.generate(&prompt, 300).await?;
    let response = response.trim();
    if response.to_uppercase().starts_with("NONE") || !response.contains('|') {
        return Ok(format!("No new synthesis extractable from '{}'", target.topic));
    }

    let parts: Vec<&str> = response.splitn(3, '|').collect();
    if parts.len() != 3 {
        return Ok(format!("Malformed synthesis output: {}", response));
    }
    let kind = parts[0].trim().to_lowercase();
    let title = parts[1].trim().to_string();
    let claim = parts[2].trim().to_string();
    if title.is_empty() || claim.len() < 20 {
        return Ok("Synthesis too sparse — discarded".into());
    }

    let cognitive_type = match kind.as_str() {
        "insight" => NODE_TYPE_INSIGHT,
        "hypothesis" => NODE_TYPE_HYPOTHESIS,
        "strategy" => NODE_TYPE_STRATEGY,
        _ => NODE_TYPE_INSIGHT,
    };

    // Create the thinking node
    let input = CreateNodeInput {
        title: title.clone(),
        content: claim,
        domain: "synthesis".into(),
        topic: target.topic.clone(),
        tags: vec!["synthesis".into(), kind.clone(), "circuit".into()],
        node_type: cognitive_type.to_string(),
        source_type: "synthesis".into(),
        source_url: None,
    };
    let created = db.create_node(input).await?;

    // Stamp synthesized_by_brain + cognitive_type + confidence
    let created_id = created.id.clone();
    let cog_type = cognitive_type.to_string();
    db.with_conn(move |conn| {
        conn.execute(
            "UPDATE nodes SET synthesized_by_brain = 1, cognitive_type = ?1, confidence = 0.6 WHERE id = ?2",
            params![cog_type, created_id],
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(())
    }).await?;

    // derived_from edges to source cluster nodes
    let mut links = 0u32;
    for src in cluster.iter().take(5) {
        let edge_id = format!("edges:{}", uuid::Uuid::now_v7());
        let now = chrono::Utc::now().to_rfc3339();
        let src_id = src.id.clone();
        let created_id = created.id.clone();
        let _ = db.with_conn(move |conn| {
            conn.execute(
                "INSERT INTO edges (id, source_id, target_id, relation_type, strength, \
                 discovered_by, evidence, animated, created_at, traversal_count) \
                 VALUES (?1, ?2, ?3, 'derived_from', 0.85, 'circuit', 'Cluster source for synthesis', 0, ?4, 0)",
                params![edge_id, created_id, src_id, now],
            ).map_err(|e| BrainError::Database(e.to_string()))?;
            Ok(())
        }).await;
        links += 1;
    }

    Ok(format!("Synthesised {} for topic '{}' ({} links)", kind, target.topic, links))
}

// =========================================================================
// CIRCUIT A — meta_reflection
// =========================================================================
//
// Read the recent autonomy_circuit_log, summarise what's been happening,
// and ask the LLM to suggest research missions for the brain to pursue.
// Saves up to 3 missions to research_missions table.

async fn circuit_meta_reflection(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    #[derive(Debug)]
    struct LogRow {
        circuit_name: String,
        status: String,
        result: String,
    }
    let recent: Vec<LogRow> = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT circuit_name, status, result FROM autonomy_circuit_log \
             ORDER BY started_at DESC LIMIT 30"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok(LogRow {
                circuit_name: row.get(0)?,
                status: row.get(1)?,
                result: row.get(2)?,
            })
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| BrainError::Database(e.to_string()))?);
        }
        Ok(results)
    }).await?;

    if recent.is_empty() {
        return Ok("No circuit history yet — meta_reflection skipped".into());
    }

    let mut summary = String::new();
    for r in &recent {
        summary.push_str(&format!("- [{}] {}: {}\n", r.status, r.circuit_name, r.result));
        if summary.len() > 4000 { break; }
    }

    // FAST tier: meta_reflection just summarises a log and suggests topics
    let llm = crate::commands::ai::get_llm_client_fast(db);
    let prompt = format!(
        "Below is a log of recent self-improvement cycles run by a personal knowledge brain. \
         Identify 1-3 research topics the brain should pursue next that would best fill its gaps \
         or strengthen its weakest areas. Output one topic per line, no numbering or preamble. \
         If everything looks healthy, output just NONE.\n\nLog:\n{}",
        summary
    );

    let response = llm.generate(&prompt, 300).await?;
    if response.trim().to_uppercase().starts_with("NONE") {
        return Ok(format!("Reflected on {} cycles, no new missions needed", recent.len()));
    }

    let mut created = 0u32;
    let now = chrono::Utc::now().to_rfc3339();
    for line in response.lines().take(3) {
        let topic = line.trim().trim_start_matches('-').trim().to_string();
        if topic.len() < 5 || topic.len() > 200 { continue; }
        let mission_id = format!("research_missions:{}", uuid::Uuid::now_v7());
        let now_c = now.clone();
        let topic_c = topic.clone();
        let _ = db.with_conn(move |conn| {
            conn.execute(
                "INSERT INTO research_missions (id, topic, status, description, priority, created_at) \
                 VALUES (?1, ?2, 'pending', '', 1, ?3)",
                params![mission_id, topic_c, now_c],
            ).map_err(|e| BrainError::Database(e.to_string()))?;
            Ok(())
        }).await;
        created += 1;
    }

    Ok(format!("Reflected on {} cycles → {} new research missions", recent.len(), created))
}

// =========================================================================
// CIRCUIT F — curiosity_gap_fill
// =========================================================================
//
// Pull from the existing curiosity queue + gap analysis, pick one topic,
// research it via the existing ingest pipeline.

async fn circuit_curiosity_gap_fill(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    let queue = crate::learning::curiosity::generate_curiosity_queue(db).await?;
    if queue.is_empty() {
        return Ok("Curiosity queue empty — circuit skipped".into());
    }

    // Filter out already-researched topics
    let logged: Vec<String> = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT topic FROM learning_log"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            row.get::<_, String>(0)
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| BrainError::Database(e.to_string()))?);
        }
        Ok(results)
    }).await?;
    let researched: std::collections::HashSet<String> = logged.iter()
        .map(|r| r.to_lowercase())
        .collect();

    let next = queue.iter()
        .find(|c| !researched.contains(&c.topic.to_lowercase()))
        .map(|c| c.topic.clone());

    let topic = match next {
        Some(t) => t,
        None => return Ok("All curiosity topics already researched".into()),
    };

    log::info!("curiosity_gap_fill researching: {}", topic);

    let nodes = match crate::commands::ingest::research_topic_inner(db, &topic).await {
        Ok(n) => n,
        Err(e) => return Ok(format!("Research failed for '{}': {}", topic, e)),
    };

    // Log the run
    let log_id = format!("learning_log:{}", uuid::Uuid::now_v7());
    let now = chrono::Utc::now().to_rfc3339();
    let topic_c = topic.clone();
    let nodes_len = nodes.len() as u64;
    let _ = db.with_conn(move |conn| {
        conn.execute(
            "INSERT INTO learning_log (id, topic, content, learned_at, source) \
             VALUES (?1, ?2, '', ?3, 'circuit')",
            params![log_id, topic_c, now],
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(())
    }).await;

    Ok(format!("Researched '{}' → {} nodes", topic, nodes_len))
}

// =========================================================================
// CIRCUIT H — compression_cycle  (Phase 1.4)
// =========================================================================
//
// Hierarchical compression. Picks one large topic cluster (50+ nodes), asks
// the LLM to write a single dense summary that covers all of them, creates
// a `summary_cluster` thinking node containing that summary, and stamps
// every source node's `compression_parent` with the new cluster's id.
//
// Effect: a 50-node topic becomes queryable as one node for most purposes,
// while the originals stay accessible for drill-down. Over many cycles the
// active query layer (the "hot" set) collapses by ~10x without losing any
// information.
//
// Cap: one cluster per cycle. Each cycle is ~1 LLM call + ~50 row updates,
// so it stays under a minute.

async fn circuit_compression_cycle(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    #[derive(Debug)]
    struct TopicGroup { topic: String, count: u64 }

    // Pick a topic with at least 50 nodes that hasn't been compressed yet.
    let topics: Vec<TopicGroup> = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT topic, COUNT(*) AS count FROM nodes \
             WHERE topic != '' AND (compression_parent IS NULL OR compression_parent = '') \
             GROUP BY topic ORDER BY count DESC LIMIT 100"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok(TopicGroup {
                topic: row.get(0)?,
                count: row.get(1)?,
            })
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| BrainError::Database(e.to_string()))?);
        }
        Ok(results)
    }).await?;

    let target = topics.into_iter().find(|t| t.count >= 50);
    let target = match target {
        Some(t) => t,
        None => return Ok("No uncompressed topics with 50+ nodes — skipped".into()),
    };

    log::info!("compression_cycle: targeting topic '{}' ({} nodes)", target.topic, target.count);

    #[derive(Debug)]
    #[allow(dead_code)]
    struct ClusterNode {
        id: String,
        title: String,
        summary: String,
        domain: String,
        quality_score: f64,
    }

    // Take up to 60 highest-quality nodes from the topic so the LLM has
    // a representative slice to summarize.
    let topic_clone = target.topic.clone();
    let cluster: Vec<ClusterNode> = db.with_conn(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, title, summary, domain, quality_score FROM nodes \
             WHERE topic = ?1 AND (compression_parent IS NULL OR compression_parent = '') \
             ORDER BY quality_score DESC LIMIT 60"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map(params![topic_clone], |row| {
            Ok(ClusterNode {
                id: row.get(0)?,
                title: row.get(1)?,
                summary: row.get(2)?,
                domain: row.get(3)?,
                quality_score: row.get(4)?,
            })
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| BrainError::Database(e.to_string()))?);
        }
        Ok(results)
    }).await?;

    if cluster.len() < 50 {
        return Ok(format!(
            "Topic '{}' had {} qualifying nodes after filtering — skipped",
            target.topic, cluster.len()
        ));
    }

    // Build the summarisation corpus (cap to ~6KB to fit Ollama 8K context).
    let mut corpus = String::new();
    for n in &cluster {
        if corpus.len() > 6000 { break; }
        corpus.push_str(&format!("- [{}] {}: {}\n", n.domain, n.title, n.summary));
    }

    // DEEP tier: dense summary writing benefits hugely from the bigger model
    let llm = crate::commands::ai::get_llm_client_deep(db);
    let prompt = format!(
        "You are compressing a cluster of {} related knowledge nodes from the topic '{}' into \
         a single dense summary. Write 4-8 sentences that capture the most important concepts, \
         relationships, decisions, and patterns from these nodes. Be specific and information-dense \
         — this is a compressed reference, not a marketing blurb. No preamble, just the summary text.\n\n\
         Nodes:\n{}",
        cluster.len(), target.topic, corpus
    );

    let summary_text = llm.generate(&prompt, 800).await?;
    let summary_text = summary_text.trim().to_string();
    if summary_text.len() < 100 {
        return Ok(format!("LLM produced too-short summary for '{}' ({} chars) — discarded", target.topic, summary_text.len()));
    }

    // Determine the dominant domain (most common in the cluster) for the
    // new summary_cluster node.
    let mut domain_counts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    for n in &cluster {
        *domain_counts.entry(n.domain.clone()).or_insert(0) += 1;
    }
    let dominant_domain = domain_counts.into_iter()
        .max_by_key(|(_, c)| *c)
        .map(|(d, _)| d)
        .unwrap_or_else(|| "synthesis".to_string());

    // Create the summary_cluster node
    let title = format!("Summary cluster: {}", target.topic);
    let input = CreateNodeInput {
        title,
        content: summary_text.clone(),
        domain: dominant_domain.clone(),
        topic: target.topic.clone(),
        tags: vec!["summary_cluster".into(), "compression".into(), "circuit".into()],
        node_type: NODE_TYPE_SUMMARY_CLUSTER.to_string(),
        source_type: "synthesis".into(),
        source_url: None,
    };
    let created = db.create_node(input).await?;

    // Stamp synthesized markers on the new cluster node
    let created_id = created.id.clone();
    db.with_conn(move |conn| {
        conn.execute(
            "UPDATE nodes SET synthesized_by_brain = 1, cognitive_type = ?1, memory_tier = 'hot' WHERE id = ?2",
            params![NODE_TYPE_SUMMARY_CLUSTER, created_id],
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(())
    }).await?;

    // Stamp compression_parent on every source node + create derived_from edges
    let mut linked = 0u32;
    let cluster_id = created.id.clone();
    for src in &cluster {
        let src_id = src.id.clone();

        // Set compression_parent on the source, demote to warm tier
        let cluster_id_c = cluster_id.clone();
        let src_id_c = src_id.clone();
        let _ = db.with_conn(move |conn| {
            conn.execute(
                "UPDATE nodes SET compression_parent = ?1, memory_tier = 'warm' WHERE id = ?2",
                params![cluster_id_c, src_id_c],
            ).map_err(|e| BrainError::Database(e.to_string()))?;
            Ok(())
        }).await;

        // derived_from edge: cluster <- source
        let edge_id = format!("edges:{}", uuid::Uuid::now_v7());
        let now = chrono::Utc::now().to_rfc3339();
        let cluster_id_c = cluster_id.clone();
        let _ = db.with_conn(move |conn| {
            conn.execute(
                "INSERT INTO edges (id, source_id, target_id, relation_type, strength, \
                 discovered_by, evidence, animated, created_at, traversal_count) \
                 VALUES (?1, ?2, ?3, 'derived_from', 0.7, 'compression', 'Compressed into summary_cluster', 0, ?4, 0)",
                params![edge_id, cluster_id_c, src_id, now],
            ).map_err(|e| BrainError::Database(e.to_string()))?;
            Ok(())
        }).await;

        linked += 1;
    }

    // Log to compression_log
    let comp_log_id = format!("compression_log:{}", uuid::Uuid::now_v7());
    let now = chrono::Utc::now().to_rfc3339();
    let cluster_id_c = cluster_id.clone();
    let child_ids: Vec<String> = cluster.iter().map(|n| n.id.clone()).collect();
    let child_ids_json = serde_json::to_string(&child_ids).unwrap_or_else(|_| "[]".into());
    let _ = db.with_conn(move |conn| {
        conn.execute(
            "INSERT INTO compression_log (id, parent_id, child_ids, created_at) \
             VALUES (?1, ?2, ?3, ?4)",
            params![comp_log_id, cluster_id_c, child_ids_json, now],
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(())
    }).await;

    Ok(format!(
        "Compressed {} nodes from topic '{}' → cluster {}",
        linked, target.topic, cluster_id
    ))
}

// =========================================================================
// PHASE 2 CIRCUITS — Self-Evolving Cognition
// =========================================================================
//
// Seven new circuits that turn the brain from a memory store into a
// reasoning engine. Each is bounded (one cycle = under a minute) and
// graceful (no-ops on empty input). They depend on Phase 0 thinking
// nodes existing — the rotating scheduler ensures self_synthesis runs
// regularly so there's always grist for the validators.

// =========================================================================
// CIRCUIT — contradiction_detector
// =========================================================================
//
// Walks the most recent thinking nodes (insights and hypotheses) and
// uses an LLM to find pairs that contradict each other. For each
// contradiction it creates a `contradicts` edge with reasoning, and a
// new `contradiction` thinking node summarizing what's in conflict.

async fn circuit_contradiction_detector(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    #[derive(Debug, Clone)]
    struct ThinkingRow {
        id: String,
        title: String,
        content: String,
        topic: String,
    }

    // Pull last 30 hypothesis/insight nodes — small window so each cycle
    // is fast and the LLM has tractable input.
    let recent: Vec<ThinkingRow> = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, title, content, topic FROM nodes \
             WHERE node_type IN ('hypothesis', 'insight') \
             ORDER BY created_at DESC LIMIT 30"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok(ThinkingRow {
                id: row.get(0)?,
                title: row.get(1)?,
                content: row.get(2)?,
                topic: row.get(3)?,
            })
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| BrainError::Database(e.to_string()))?);
        }
        Ok(results)
    }).await?;

    if recent.len() < 4 {
        return Ok("Not enough thinking nodes yet for contradiction detection".into());
    }

    // Group by topic — contradictions only make sense within a topic.
    let mut by_topic: std::collections::HashMap<String, Vec<ThinkingRow>> = std::collections::HashMap::new();
    for r in &recent {
        if !r.topic.is_empty() {
            by_topic.entry(r.topic.clone()).or_default().push(r.clone());
        }
    }

    // Pick the topic with the most thinking nodes for this cycle
    let target = by_topic.into_iter().max_by_key(|(_, v)| v.len());
    let (topic, nodes_in_topic) = match target {
        Some(t) => t,
        None => return Ok("No topic-grouped thinking nodes — skipped".into()),
    };
    if nodes_in_topic.len() < 2 {
        return Ok(format!("Topic '{}' has too few thinking nodes ({}) — skipped", topic, nodes_in_topic.len()));
    }

    // Build a numbered list for the LLM
    let mut listing = String::new();
    for (i, n) in nodes_in_topic.iter().take(15).enumerate() {
        listing.push_str(&format!("{}. {}: {}\n", i + 1, n.title, short(&n.content, 200)));
    }

    // DEEP tier: contradiction detection requires careful logical reasoning
    let llm = crate::commands::ai::get_llm_client_deep(db);
    let prompt = format!(
        "Below are knowledge claims about the topic '{}'. Find pairs that DIRECTLY contradict each other \
         (one says X, the other says NOT X). Output one line per contradiction in this exact format:\n\n\
         A | B | REASON\n\n\
         where A and B are the numbers from the list and REASON is one short sentence. \
         If no real contradictions exist, output just NONE. \
         Don't invent contradictions just because two statements differ — they must be incompatible.\n\n\
         Claims:\n{}",
        topic, listing
    );

    let response = llm.generate(&prompt, 400).await?;
    if response.trim().to_uppercase().starts_with("NONE") {
        return Ok(format!("No contradictions found in topic '{}'", topic));
    }

    let mut created = 0u32;
    for line in response.lines() {
        let line = line.trim();
        if line.is_empty() || line.matches('|').count() < 2 {
            continue;
        }
        let parts: Vec<&str> = line.splitn(3, '|').collect();
        if parts.len() != 3 { continue; }
        let a_idx: usize = match parts[0].trim().parse::<usize>() { Ok(n) if n >= 1 => n - 1, _ => continue };
        let b_idx: usize = match parts[1].trim().parse::<usize>() { Ok(n) if n >= 1 => n - 1, _ => continue };
        let reason = parts[2].trim().to_string();
        if reason.len() < 10 { continue; }
        if a_idx >= nodes_in_topic.len() || b_idx >= nodes_in_topic.len() { continue; }
        if a_idx == b_idx { continue; }

        let a = &nodes_in_topic[a_idx];
        let b = &nodes_in_topic[b_idx];
        let id_a = a.id.clone();
        let id_b = b.id.clone();

        // Create the contradicts edge (bidirectional, but we only insert one
        // — graph traversal can follow either direction)
        let edge_id = format!("edges:{}", uuid::Uuid::now_v7());
        let now = chrono::Utc::now().to_rfc3339();
        let id_a_c = id_a.clone();
        let id_b_c = id_b.clone();
        let reason_c = reason.clone();
        let _ = db.with_conn(move |conn| {
            conn.execute(
                "INSERT INTO edges (id, source_id, target_id, relation_type, strength, \
                 discovered_by, evidence, animated, created_at, traversal_count) \
                 VALUES (?1, ?2, ?3, 'contradicts', 0.85, 'circuit', ?4, 1, ?5, 0)",
                params![edge_id, id_a_c, id_b_c, reason_c, now],
            ).map_err(|e| BrainError::Database(e.to_string()))?;
            Ok(())
        }).await;

        // Create a contradiction thinking node summarizing the conflict
        let title = format!("Contradiction: {} vs {}", short(&a.title, 40), short(&b.title, 40));
        let content = format!(
            "Conflict detected in topic '{}':\n\nA: {}\nB: {}\n\nWhy they conflict: {}",
            topic, a.title, b.title, reason
        );
        let input = CreateNodeInput {
            title,
            content,
            domain: "synthesis".into(),
            topic: topic.clone(),
            tags: vec!["contradiction".into(), "circuit".into()],
            node_type: NODE_TYPE_CONTRADICTION.to_string(),
            source_type: "synthesis".into(),
            source_url: None,
        };
        if let Ok(node) = db.create_node(input).await {
            let node_id = node.id.clone();
            let _ = db.with_conn(move |conn| {
                conn.execute(
                    "UPDATE nodes SET synthesized_by_brain = 1, cognitive_type = ?1, confidence = 0.7 WHERE id = ?2",
                    params![NODE_TYPE_CONTRADICTION, node_id],
                ).map_err(|e| BrainError::Database(e.to_string()))?;
                Ok(())
            }).await;
        }

        created += 1;
        if created >= 5 { break; }
    }

    Ok(format!("Detected {} contradictions in topic '{}'", created, topic))
}

// =========================================================================
// CIRCUIT — decision_memory_extractor
// =========================================================================
//
// Scans recent chat history for "we decided X because Y" / "going with Z"
// patterns. The LLM extracts each decision plus its reasoning. Creates a
// `decision` thinking node with `derived_from` edges back to the source
// chat. This is the highest-value sidekick content — Claude Code can
// later ask "what did we decide about X?" via the MCP `brain_decisions`
// tool and get exact past reasoning.

async fn circuit_decision_memory_extractor(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    #[derive(Debug)]
    struct ChatRow {
        id: String,
        content: String,
    }

    // Pull last 40 chat nodes
    let chats: Vec<ChatRow> = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, content FROM nodes \
             WHERE source_type IN ('chat_history', 'auto_sync') \
             ORDER BY created_at DESC LIMIT 40"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok(ChatRow {
                id: row.get(0)?,
                content: row.get(1)?,
            })
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| BrainError::Database(e.to_string()))?);
        }
        Ok(results)
    }).await?;

    if chats.is_empty() {
        return Ok("No chat history available".into());
    }

    // Build a corpus capped at ~6000 chars
    let mut corpus = String::new();
    let mut trigger_ids: Vec<String> = Vec::new();
    for c in &chats {
        if corpus.len() > 6000 { break; }
        let take: String = c.content.chars().take(800).collect();
        corpus.push_str(&take);
        corpus.push_str("\n---\n");
        trigger_ids.push(c.id.clone());
    }

    // FAST tier: decision extraction is structured pattern matching
    let llm = crate::commands::ai::get_llm_client_fast(db);
    let prompt = format!(
        "Below are recent conversations. Extract concrete DECISIONS that were made — moments where \
         someone chose between options or committed to an approach. For each decision output one \
         line in this exact format:\n\n\
         DECISION | REASONING\n\n\
         DECISION is one short sentence stating what was chosen. REASONING is one sentence stating why. \
         Skip vague statements, opinions, or musings — only extract real commitments. \
         If nothing qualifies, output just NONE.\n\n\
         Conversations:\n{}",
        corpus
    );

    let response = llm.generate(&prompt, 600).await?;
    if response.trim().to_uppercase().starts_with("NONE") {
        return Ok("No new decisions extracted".into());
    }

    let mut created = 0u32;
    for line in response.lines() {
        let line = line.trim();
        if line.is_empty() || !line.contains('|') { continue; }
        let parts: Vec<&str> = line.splitn(2, '|').collect();
        if parts.len() != 2 { continue; }
        let decision = parts[0].trim().to_string();
        let reasoning = parts[1].trim().to_string();
        if decision.len() < 10 || reasoning.len() < 10 { continue; }
        if decision.len() > 200 || reasoning.len() > 400 { continue; }

        // Dedupe: skip if a similar decision already exists
        let existing: Vec<String> = db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT title FROM nodes WHERE node_type = 'decision' LIMIT 100"
            ).map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt.query_map([], |row| {
                row.get::<_, String>(0)
            }).map_err(|e| BrainError::Database(e.to_string()))?;
            let mut results = Vec::new();
            for row in rows {
                results.push(row.map_err(|e| BrainError::Database(e.to_string()))?);
            }
            Ok(results)
        }).await?;
        let dup = existing.iter().any(|e| similar_text(e, &decision) > 0.6);
        if dup { continue; }

        // Create the decision thinking node
        let title = decision.clone();
        let content = format!("**Decision:** {}\n\n**Reasoning:** {}", decision, reasoning);
        let input = CreateNodeInput {
            title,
            content,
            domain: "synthesis".into(),
            topic: "decisions".into(),
            tags: vec!["decision".into(), "extracted".into(), "circuit".into()],
            node_type: NODE_TYPE_DECISION.to_string(),
            source_type: "synthesis".into(),
            source_url: None,
        };
        if let Ok(node) = db.create_node(input).await {
            let node_id = node.id.clone();
            let _ = db.with_conn(move |conn| {
                conn.execute(
                    "UPDATE nodes SET synthesized_by_brain = 1, cognitive_type = ?1, confidence = 0.75 WHERE id = ?2",
                    params![NODE_TYPE_DECISION, node_id],
                ).map_err(|e| BrainError::Database(e.to_string()))?;
                Ok(())
            }).await;

            // derived_from edges to first 3 source chats
            for src_id in trigger_ids.iter().take(3) {
                let edge_id = format!("edges:{}", uuid::Uuid::now_v7());
                let now = chrono::Utc::now().to_rfc3339();
                let node_id = node.id.clone();
                let tgt_id = src_id.clone();
                let _ = db.with_conn(move |conn| {
                    conn.execute(
                        "INSERT INTO edges (id, source_id, target_id, relation_type, strength, \
                         discovered_by, evidence, animated, created_at, traversal_count) \
                         VALUES (?1, ?2, ?3, 'derived_from', 0.7, 'circuit', 'Decision extracted from chat', 0, ?4, 0)",
                        params![edge_id, node_id, tgt_id, now],
                    ).map_err(|e| BrainError::Database(e.to_string()))?;
                    Ok(())
                }).await;
            }

            created += 1;
        }
        if created >= 5 { break; }
    }

    Ok(format!("Extracted {} new decisions from {} chats", created, chats.len()))
}

// =========================================================================
// CIRCUIT — knowledge_synthesizer
// =========================================================================
//
// Bigger sibling of self_synthesis: instead of synthesizing within a
// topic, this works at the DOMAIN level and produces a "wisdom node"
// summarizing what the brain has learned about the whole domain. Creates
// one synthesis thinking node per cycle.

async fn circuit_knowledge_synthesizer(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    #[derive(Debug)]
    struct DomainGroup { domain: String, count: u64 }

    let domains: Vec<DomainGroup> = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT domain, COUNT(*) AS count FROM nodes \
             WHERE domain != '' GROUP BY domain ORDER BY count DESC LIMIT 20"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok(DomainGroup {
                domain: row.get(0)?,
                count: row.get(1)?,
            })
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| BrainError::Database(e.to_string()))?);
        }
        Ok(results)
    }).await?;

    if domains.is_empty() {
        return Ok("No domains found".into());
    }

    // Rotate by time so we cover all domains over multiple cycles
    let idx = (chrono::Utc::now().timestamp() as usize / 3600) % domains.len();
    let target = &domains[idx];
    if target.count < 20 {
        return Ok(format!("Domain '{}' too small ({} nodes) — skipped", target.domain, target.count));
    }

    #[derive(Debug)]
    struct DomainNode {
        id: String,
        title: String,
        summary: String,
    }

    // Pull top 50 highest-quality nodes from this domain
    let domain_clone = target.domain.clone();
    let cluster: Vec<DomainNode> = db.with_conn(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, title, summary FROM nodes \
             WHERE domain = ?1 ORDER BY quality_score DESC LIMIT 50"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map(params![domain_clone], |row| {
            Ok(DomainNode {
                id: row.get(0)?,
                title: row.get(1)?,
                summary: row.get(2)?,
            })
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| BrainError::Database(e.to_string()))?);
        }
        Ok(results)
    }).await?;

    if cluster.len() < 10 {
        return Ok(format!("Domain '{}' has too few quality nodes — skipped", target.domain));
    }

    let mut corpus = String::new();
    for n in cluster.iter().take(30) {
        if corpus.len() > 6000 { break; }
        corpus.push_str(&format!("- {}: {}\n", n.title, short(&n.summary, 200)));
    }

    // DEEP tier: domain-level wisdom synthesis is the highest-quality task
    let llm = crate::commands::ai::get_llm_client_deep(db);
    let prompt = format!(
        "Below are knowledge nodes from the '{}' domain. Write a single dense WISDOM summary \
         (6-10 sentences) capturing the most important concepts, patterns, decisions, and insights \
         the brain has learned about this domain. Be specific. No preamble. \
         End with a TAGS line: TAGS: tag1, tag2, tag3 (3-6 tags).\n\n\
         Nodes:\n{}",
        target.domain, corpus
    );

    let response = llm.generate(&prompt, 800).await?;
    let response = response.trim().to_string();
    if response.len() < 200 {
        return Ok(format!("LLM produced too-short wisdom for '{}'", target.domain));
    }

    // Split off TAGS line
    let (body, tags) = if let Some(tag_pos) = response.rfind("TAGS:") {
        let body_str = response[..tag_pos].trim().to_string();
        let tag_str = response[tag_pos + 5..].trim();
        let tag_vec: Vec<String> = tag_str
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty() && s.len() < 30)
            .take(8)
            .collect();
        (body_str, tag_vec)
    } else {
        (response, vec![target.domain.clone(), "wisdom".into()])
    };

    let mut all_tags = vec!["wisdom".into(), "synthesis".into(), "circuit".into()];
    all_tags.extend(tags);

    let title = format!("Wisdom: {} domain", target.domain);
    let input = CreateNodeInput {
        title,
        content: body,
        domain: "synthesis".into(),
        topic: format!("{}-wisdom", target.domain),
        tags: all_tags,
        node_type: NODE_TYPE_INSIGHT.to_string(),
        source_type: "synthesis".into(),
        source_url: None,
    };
    let created = db.create_node(input).await?;

    let created_id = created.id.clone();
    let _ = db.with_conn(move |conn| {
        conn.execute(
            "UPDATE nodes SET synthesized_by_brain = 1, cognitive_type = ?1, confidence = 0.7 WHERE id = ?2",
            params![NODE_TYPE_INSIGHT, created_id],
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(())
    }).await;

    // derived_from edges to top 5 source nodes
    let mut links = 0u32;
    for src in cluster.iter().take(5) {
        let edge_id = format!("edges:{}", uuid::Uuid::now_v7());
        let now = chrono::Utc::now().to_rfc3339();
        let created_id = created.id.clone();
        let tgt_id = src.id.clone();
        let _ = db.with_conn(move |conn| {
            conn.execute(
                "INSERT INTO edges (id, source_id, target_id, relation_type, strength, \
                 discovered_by, evidence, animated, created_at, traversal_count) \
                 VALUES (?1, ?2, ?3, 'derived_from', 0.85, 'circuit', 'Source for domain wisdom', 0, ?4, 0)",
                params![edge_id, created_id, tgt_id, now],
            ).map_err(|e| BrainError::Database(e.to_string()))?;
            Ok(())
        }).await;
        links += 1;
    }

    Ok(format!("Synthesised wisdom for '{}' domain ({} links)", target.domain, links))
}

// =========================================================================
// CIRCUIT — self_assessment
// =========================================================================
//
// The brain looking at itself. Aggregates IQ breakdown, recent circuit
// success rate, and node distribution to identify the weakest dimension.
// Generates 1-3 research_mission entries targeting that gap, plus an
// `insight` thinking node recording the assessment.

async fn circuit_self_assessment(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    // Get the current IQ breakdown (this is the brain looking in the mirror)
    let trends = match crate::analysis::trends::analyze_trends(db).await {
        Ok(t) => t,
        Err(e) => return Ok(format!("Could not run analyze_trends: {}", e)),
    };

    // Find the weakest IQ component
    let breakdown = &trends.iq_breakdown;
    let components: Vec<(&str, f64, f64)> = vec![
        ("quality (foundation)", breakdown.quality, 25.0),
        ("connectivity (foundation)", breakdown.connectivity, 20.0),
        ("freshness (foundation)", breakdown.freshness, 20.0),
        ("diversity (foundation)", breakdown.diversity, 15.0),
        ("coverage (foundation)", breakdown.coverage, 10.0),
        ("volume (foundation)", breakdown.volume, 10.0),
        ("depth (intelligence)", breakdown.depth, 20.0),
        ("cross_domain (intelligence)", breakdown.cross_domain, 20.0),
        ("semantic (intelligence)", breakdown.semantic, 20.0),
        ("research_ratio (intelligence)", breakdown.research_ratio, 20.0),
        ("coherence (intelligence)", breakdown.coherence, 10.0),
        ("high_quality_pct (intelligence)", breakdown.high_quality_pct, 10.0),
        ("self_improvement_velocity (meta)", breakdown.self_improvement_velocity, 25.0),
        ("prediction_accuracy (meta)", breakdown.prediction_accuracy, 25.0),
        ("novel_insight_rate (meta)", breakdown.novel_insight_rate, 20.0),
        ("autonomy_independence (meta)", breakdown.autonomy_independence, 15.0),
        ("user_model_accuracy (meta)", breakdown.user_model_accuracy, 15.0),
    ];

    // Find weakest by ratio (current / max)
    let weakest = components.iter()
        .min_by(|a, b| {
            let ra = a.1 / a.2;
            let rb = b.1 / b.2;
            ra.partial_cmp(&rb).unwrap_or(std::cmp::Ordering::Equal)
        })
        .cloned();
    let (weakest_name, weakest_pts, weakest_max) = match weakest {
        Some(t) => t,
        None => return Ok("No IQ components to assess".into()),
    };

    let weakest_ratio = weakest_pts / weakest_max;
    let assessment = format!(
        "Current Brain IQ: {:.0}/300. Weakest dimension: {} at {:.0}/{:.0} ({:.0}% saturation). \
         Total nodes: {:.0}, total synapses derived from connectivity score.",
        trends.brain_iq,
        weakest_name,
        weakest_pts,
        weakest_max,
        weakest_ratio * 100.0,
        // Approximate total_nodes from volume formula reverse
        10f64.powf((breakdown.volume / 10.0) * 5.7),
    );

    // Generate targeted research missions if the weak area benefits from research
    // FAST tier: just suggesting topic strings, no heavy reasoning
    let llm = crate::commands::ai::get_llm_client_fast(db);
    let prompt = format!(
        "The brain's weakest IQ component is '{}'. Suggest 1-3 specific research topics that, \
         if studied, would improve this dimension. Output one topic per line, no numbering, no preamble. \
         If research wouldn't help, output just NONE.\n\n\
         Context: {}",
        weakest_name, assessment
    );

    let mut missions_created = 0u32;
    if let Ok(response) = llm.generate(&prompt, 200).await {
        if !response.trim().to_uppercase().starts_with("NONE") {
            let now = chrono::Utc::now().to_rfc3339();
            for line in response.lines().take(3) {
                let topic = line.trim().trim_start_matches('-').trim().to_string();
                if topic.len() < 5 || topic.len() > 200 { continue; }
                let mission_id = format!("research_missions:{}", uuid::Uuid::now_v7());
                let now_c = now.clone();
                let topic_c = topic.clone();
                let _ = db.with_conn(move |conn| {
                    conn.execute(
                        "INSERT INTO research_missions (id, topic, status, description, priority, created_at) \
                         VALUES (?1, ?2, 'pending', '', 2, ?3)",
                        params![mission_id, topic_c, now_c],
                    ).map_err(|e| BrainError::Database(e.to_string()))?;
                    Ok(())
                }).await;
                missions_created += 1;
            }
        }
    }

    // Record the assessment as an insight node
    let title = format!("Self-assessment: weakest = {}", weakest_name);
    let input = CreateNodeInput {
        title,
        content: assessment,
        domain: "synthesis".into(),
        topic: "brain-self-assessment".into(),
        tags: vec!["self_assessment".into(), "circuit".into(), "meta".into()],
        node_type: NODE_TYPE_INSIGHT.to_string(),
        source_type: "synthesis".into(),
        source_url: None,
    };
    if let Ok(node) = db.create_node(input).await {
        let node_id = node.id.clone();
        let _ = db.with_conn(move |conn| {
            conn.execute(
                "UPDATE nodes SET synthesized_by_brain = 1, cognitive_type = ?1, confidence = 0.85 WHERE id = ?2",
                params![NODE_TYPE_INSIGHT, node_id],
            ).map_err(|e| BrainError::Database(e.to_string()))?;
            Ok(())
        }).await;
    }

    Ok(format!(
        "Self-assessed: weakest = {} ({:.0}% saturation), spawned {} research missions",
        weakest_name, weakest_ratio * 100.0, missions_created
    ))
}

// =========================================================================
// CIRCUIT — prediction_validator
// =========================================================================
//
// Walks `prediction` thinking nodes older than 24 hours. For each, asks
// the LLM whether the prediction has been borne out by what's happened
// in the brain since (recent nodes about the same topic). Updates the
// prediction's confidence and creates evidence_for/evidence_against
// edges to the supporting nodes.

async fn circuit_prediction_validator(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    let one_day_ago = (chrono::Utc::now() - chrono::Duration::hours(24)).to_rfc3339();

    #[derive(Debug)]
    struct PredNode {
        id: String,
        title: String,
        content: String,
        topic: String,
        confidence: Option<f32>,
    }

    let cutoff = one_day_ago.clone();
    let predictions: Vec<PredNode> = db.with_conn(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, title, content, topic, confidence FROM nodes \
             WHERE node_type = 'prediction' AND created_at < ?1 \
             ORDER BY created_at ASC LIMIT 10"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map(params![cutoff], |row| {
            Ok(PredNode {
                id: row.get(0)?,
                title: row.get(1)?,
                content: row.get(2)?,
                topic: row.get(3)?,
                confidence: row.get(4)?,
            })
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| BrainError::Database(e.to_string()))?);
        }
        Ok(results)
    }).await?;

    if predictions.is_empty() {
        return Ok("No predictions older than 24h to validate".into());
    }

    // DEEP tier: prediction validation requires nuanced outcome analysis
    let llm = crate::commands::ai::get_llm_client_deep(db);
    let mut validated = 0u32;
    let mut confirmed = 0u32;
    let mut weakened = 0u32;

    for pred in &predictions {
        let pred_id = pred.id.clone();

        // Pull related recent nodes from the same topic
        #[derive(Debug)]
        struct RelatedNode {
            id: String,
            title: String,
            summary: String,
        }
        let topic_clone = pred.topic.clone();
        let related: Vec<RelatedNode> = db.with_conn(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, title, summary FROM nodes \
                 WHERE topic = ?1 AND node_type != 'prediction' \
                 ORDER BY created_at DESC LIMIT 8"
            ).map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt.query_map(params![topic_clone], |row| {
                Ok(RelatedNode {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    summary: row.get(2)?,
                })
            }).map_err(|e| BrainError::Database(e.to_string()))?;
            let mut results = Vec::new();
            for row in rows {
                results.push(row.map_err(|e| BrainError::Database(e.to_string()))?);
            }
            Ok(results)
        }).await.unwrap_or_default();

        if related.is_empty() {
            continue;
        }

        let mut evidence_listing = String::new();
        for (i, n) in related.iter().enumerate() {
            evidence_listing.push_str(&format!("{}. {}: {}\n", i + 1, n.title, short(&n.summary, 150)));
        }

        let prompt = format!(
            "PREDICTION: {}\n{}\n\nEVIDENCE (recent knowledge about the same topic):\n{}\n\n\
             Based on the evidence, has the prediction been confirmed, weakened, or is there \
             not enough information yet? Output one line in this format:\n\n\
             VERDICT | EVIDENCE_NUMBERS\n\n\
             VERDICT must be one of: CONFIRMED | WEAKENED | INCONCLUSIVE\n\
             EVIDENCE_NUMBERS is comma-separated indices from the list above that support the verdict (or NONE).",
            pred.title, short(&pred.content, 400), evidence_listing
        );

        let response = match llm.generate(&prompt, 200).await {
            Ok(r) => r,
            Err(_) => continue,
        };
        let line = response.trim().lines().next().unwrap_or("").to_string();
        if !line.contains('|') { continue; }
        let parts: Vec<&str> = line.splitn(2, '|').collect();
        let verdict = parts[0].trim().to_uppercase();
        let nums_str = if parts.len() > 1 { parts[1].trim() } else { "" };

        let evidence_indices: Vec<usize> = nums_str
            .split(',')
            .filter_map(|s| s.trim().parse::<usize>().ok())
            .filter(|n| *n >= 1 && *n <= related.len())
            .map(|n| n - 1)
            .collect();

        let old_confidence = pred.confidence.unwrap_or(0.5);
        let (new_confidence, edge_type) = match verdict.as_str() {
            "CONFIRMED" => {
                confirmed += 1;
                ((old_confidence + 0.15).min(0.99), "evidence_for")
            }
            "WEAKENED" => {
                weakened += 1;
                ((old_confidence - 0.15).max(0.0), "evidence_against")
            }
            _ => continue,
        };

        // Update the prediction's confidence
        let pred_id_c = pred_id.clone();
        let _ = db.with_conn(move |conn| {
            conn.execute(
                "UPDATE nodes SET confidence = ?1 WHERE id = ?2",
                params![new_confidence, pred_id_c],
            ).map_err(|e| BrainError::Database(e.to_string()))?;
            Ok(())
        }).await;

        // Create evidence edges
        for idx in evidence_indices.iter().take(3) {
            let ev_id = related[*idx].id.clone();
            if !ev_id.is_empty() {
                let edge_id = format!("edges:{}", uuid::Uuid::now_v7());
                let now = chrono::Utc::now().to_rfc3339();
                let pred_id_c = pred_id.clone();
                let edge_type_c = edge_type.to_string();
                let verdict_c = verdict.clone();
                let _ = db.with_conn(move |conn| {
                    conn.execute(
                        "INSERT INTO edges (id, source_id, target_id, relation_type, strength, \
                         discovered_by, evidence, animated, created_at, traversal_count) \
                         VALUES (?1, ?2, ?3, ?4, 0.8, 'circuit', ?5, 1, ?6, 0)",
                        params![edge_id, ev_id, pred_id_c, edge_type_c, format!("Validates prediction: {}", verdict_c), now],
                    ).map_err(|e| BrainError::Database(e.to_string()))?;
                    Ok(())
                }).await;
            }
        }

        validated += 1;
        if validated >= 5 { break; }
    }

    Ok(format!(
        "Validated {} predictions ({} confirmed, {} weakened)",
        validated, confirmed, weakened
    ))
}

// =========================================================================
// CIRCUIT — hypothesis_tester
// =========================================================================
//
// Walks recent `hypothesis` thinking nodes. For each, runs vector_search
// for related new knowledge, asks the LLM whether each result supports or
// weakens the hypothesis, and updates confidence.

async fn circuit_hypothesis_tester(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    #[derive(Debug)]
    struct HypNode {
        id: String,
        title: String,
        content: String,
        confidence: Option<f32>,
    }

    let hyps: Vec<HypNode> = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, title, content, confidence FROM nodes \
             WHERE node_type = 'hypothesis' \
             ORDER BY created_at DESC LIMIT 10"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok(HypNode {
                id: row.get(0)?,
                title: row.get(1)?,
                content: row.get(2)?,
                confidence: row.get(3)?,
            })
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| BrainError::Database(e.to_string()))?);
        }
        Ok(results)
    }).await?;

    if hyps.is_empty() {
        return Ok("No hypotheses to test".into());
    }

    // DEEP tier: hypothesis testing is multi-step evidence reasoning
    let llm = crate::commands::ai::get_llm_client_deep(db);
    let mut tested = 0u32;

    for hyp in &hyps {
        let hyp_id = hyp.id.clone();

        // Find evidence via vector search on the hypothesis content
        let client = crate::embeddings::OllamaClient::new(
            db.config.ollama_url.clone(),
            db.config.embedding_model.clone(),
        );
        if !client.health_check().await {
            return Ok("Ollama not available — hypothesis tester deferred".into());
        }
        let query_emb = match client.generate_embedding(&hyp.content).await {
            Ok(e) => e,
            Err(_) => continue,
        };
        let related = db.vector_search(query_emb, 6).await.unwrap_or_default();
        // Skip the hypothesis itself
        let related: Vec<_> = related.into_iter().filter(|r| r.node.id != hyp_id).collect();
        if related.len() < 2 { continue; }

        let mut listing = String::new();
        for (i, r) in related.iter().take(5).enumerate() {
            listing.push_str(&format!("{}. {}: {}\n", i + 1, r.node.title, short(&r.node.summary, 150)));
        }

        let prompt = format!(
            "HYPOTHESIS: {}\n{}\n\nRELATED KNOWLEDGE:\n{}\n\n\
             For each related item, mark whether it supports (S), weakens (W), or is neutral (N) \
             toward the hypothesis. Output one line in this format:\n\n\
             VERDICT | SUPPORTING_NUMBERS | WEAKENING_NUMBERS\n\n\
             VERDICT is one word: SUPPORTED | WEAKENED | INCONCLUSIVE.\n\
             SUPPORTING_NUMBERS and WEAKENING_NUMBERS are comma-separated indices (or NONE).",
            hyp.title, short(&hyp.content, 400), listing
        );

        let response = match llm.generate(&prompt, 200).await {
            Ok(r) => r,
            Err(_) => continue,
        };
        let line = response.trim().lines().next().unwrap_or("").to_string();
        let parts: Vec<&str> = line.splitn(3, '|').collect();
        if parts.len() < 3 { continue; }
        let verdict = parts[0].trim().to_uppercase();
        let supporting: Vec<usize> = parts[1].split(',').filter_map(|s| s.trim().parse().ok()).filter(|n: &usize| *n >= 1 && *n <= related.len()).map(|n| n - 1).collect();
        let weakening: Vec<usize> = parts[2].split(',').filter_map(|s| s.trim().parse().ok()).filter(|n: &usize| *n >= 1 && *n <= related.len()).map(|n| n - 1).collect();

        let old_confidence = hyp.confidence.unwrap_or(0.5);
        let new_confidence = match verdict.as_str() {
            "SUPPORTED" => (old_confidence + 0.10).min(0.99),
            "WEAKENED" => (old_confidence - 0.10).max(0.01),
            _ => old_confidence,
        };

        // Update confidence
        let hyp_id_c = hyp_id.clone();
        let _ = db.with_conn(move |conn| {
            conn.execute(
                "UPDATE nodes SET confidence = ?1 WHERE id = ?2",
                params![new_confidence, hyp_id_c],
            ).map_err(|e| BrainError::Database(e.to_string()))?;
            Ok(())
        }).await;

        // Create edges for supporting + weakening evidence
        for idx in supporting.iter().take(3) {
            let id_str = related[*idx].node.id.clone();
            if !id_str.is_empty() {
                let edge_id = format!("edges:{}", uuid::Uuid::now_v7());
                let now = chrono::Utc::now().to_rfc3339();
                let hyp_id_c = hyp_id.clone();
                let _ = db.with_conn(move |conn| {
                    conn.execute(
                        "INSERT INTO edges (id, source_id, target_id, relation_type, strength, \
                         discovered_by, evidence, animated, created_at, traversal_count) \
                         VALUES (?1, ?2, ?3, 'evidence_for', 0.75, 'circuit', 'Supports hypothesis', 1, ?4, 0)",
                        params![edge_id, id_str, hyp_id_c, now],
                    ).map_err(|e| BrainError::Database(e.to_string()))?;
                    Ok(())
                }).await;
            }
        }
        for idx in weakening.iter().take(3) {
            let id_str = related[*idx].node.id.clone();
            if !id_str.is_empty() {
                let edge_id = format!("edges:{}", uuid::Uuid::now_v7());
                let now = chrono::Utc::now().to_rfc3339();
                let hyp_id_c = hyp_id.clone();
                let _ = db.with_conn(move |conn| {
                    conn.execute(
                        "INSERT INTO edges (id, source_id, target_id, relation_type, strength, \
                         discovered_by, evidence, animated, created_at, traversal_count) \
                         VALUES (?1, ?2, ?3, 'evidence_against', 0.75, 'circuit', 'Weakens hypothesis', 1, ?4, 0)",
                        params![edge_id, id_str, hyp_id_c, now],
                    ).map_err(|e| BrainError::Database(e.to_string()))?;
                    Ok(())
                }).await;
            }
        }

        tested += 1;
        if tested >= 5 { break; }
    }

    Ok(format!("Tested {} hypotheses", tested))
}

// =========================================================================
// CIRCUIT — code_pattern_extractor
// =========================================================================
//
// Walks recent code_snippet nodes and uses the LLM to extract project-wide
// conventions (naming, structure, error handling). Stores them as
// user_cognition rules with pattern_type='coding_style'.

async fn circuit_code_pattern_extractor(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    #[derive(Debug)]
    struct CodeNode {
        id: String,
        title: String,
        content: String,
    }

    let snippets: Vec<CodeNode> = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, title, content FROM nodes \
             WHERE node_type = 'code_snippet' \
             ORDER BY created_at DESC LIMIT 25"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok(CodeNode {
                id: row.get(0)?,
                title: row.get(1)?,
                content: row.get(2)?,
            })
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row.map_err(|e| BrainError::Database(e.to_string()))?);
        }
        Ok(results)
    }).await?;

    if snippets.len() < 5 {
        return Ok("Not enough code snippets to extract patterns".into());
    }

    let mut corpus = String::new();
    let mut trigger_ids: Vec<String> = Vec::new();
    for s in &snippets {
        if corpus.len() > 5500 { break; }
        let take: String = s.content.chars().take(600).collect();
        corpus.push_str(&format!("// {}\n{}\n\n", s.title, take));
        trigger_ids.push(s.id.clone());
    }

    // FAST tier: code convention extraction is structured pattern matching
    let llm = crate::commands::ai::get_llm_client_fast(db);
    let prompt = format!(
        "Below are code snippets from a real project. Extract 1-5 specific coding conventions \
         the author follows (naming, structure, error handling, function design). Output one \
         convention per line in this format:\n\n\
         RULE\n\n\
         Each RULE is one short sentence. Skip generic best practices — only list patterns \
         clearly evidenced in the snippets. If no clear conventions emerge, output just NONE.\n\n\
         Snippets:\n{}",
        corpus
    );

    let response = llm.generate(&prompt, 400).await?;
    if response.trim().to_uppercase().starts_with("NONE") {
        return Ok("No clear code conventions extracted".into());
    }

    let mut new_rules = 0u32;
    let mut confirmed = 0u32;
    let now = chrono::Utc::now().to_rfc3339();

    for line in response.lines() {
        let rule = line.trim().trim_start_matches('-').trim().to_string();
        if rule.len() < 15 || rule.len() > 280 { continue; }

        // Dedupe against existing coding_style rules
        #[derive(Debug)]
        struct ExistingRule {
            id: String,
            extracted_rule: String,
            times_confirmed: u32,
        }
        let existing: Vec<ExistingRule> = db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, extracted_rule, times_confirmed FROM user_cognition \
                 WHERE pattern_type = 'coding_style' LIMIT 100"
            ).map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt.query_map([], |row| {
                Ok(ExistingRule {
                    id: row.get(0)?,
                    extracted_rule: row.get(1)?,
                    times_confirmed: row.get(2)?,
                })
            }).map_err(|e| BrainError::Database(e.to_string()))?;
            let mut results = Vec::new();
            for row in rows {
                results.push(row.map_err(|e| BrainError::Database(e.to_string()))?);
            }
            Ok(results)
        }).await?;

        let dup = existing.iter().find(|e| similar_text(&e.extracted_rule, &rule) > 0.55);
        if let Some(d) = dup {
            // Confirm the existing rule
            let rid = d.id.clone();
            let new_conf = ((d.times_confirmed + 1) as f32 / (d.times_confirmed + 2) as f32).min(0.99);
            let new_count = d.times_confirmed + 1;
            let now_c = now.clone();
            let _ = db.with_conn(move |conn| {
                conn.execute(
                    "UPDATE user_cognition SET times_confirmed = ?1, confidence = ?2, timestamp = ?3 WHERE id = ?4",
                    params![new_count, new_conf, now_c, rid],
                ).map_err(|e| BrainError::Database(e.to_string()))?;
                Ok(())
            }).await;
            confirmed += 1;
        } else {
            // Insert new rule
            let cog_id = format!("user_cognition:{}", uuid::Uuid::now_v7());
            let trigger_json = serde_json::to_string(&trigger_ids.iter().take(3).collect::<Vec<_>>()).unwrap_or_else(|_| "[]".into());
            let linked_json = "[]".to_string();
            let now_c = now.clone();
            let rule_c = rule.clone();
            let _ = db.with_conn(move |conn| {
                conn.execute(
                    "INSERT INTO user_cognition (id, timestamp, trigger_node_ids, pattern_type, \
                     extracted_rule, structured_rule, confidence, times_confirmed, times_contradicted, \
                     embedding, linked_to_nodes) \
                     VALUES (?1, ?2, ?3, 'coding_style', ?4, NULL, 0.55, 1, 0, NULL, ?5)",
                    params![cog_id, now_c, trigger_json, rule_c, linked_json],
                ).map_err(|e| BrainError::Database(e.to_string()))?;
                Ok(())
            }).await;
            new_rules += 1;
        }
    }

    Ok(format!(
        "{} new + {} confirmed code conventions from {} snippets",
        new_rules, confirmed, snippets.len()
    ))
}

// =========================================================================
// CIRCUIT — synapse_prune  (Phase 4.5)
// =========================================================================
//
// Walks edges and deletes the truly low-value ones to keep the graph
// healthy as it scales. Criteria for deletion:
//   - strength < 0.3 (weakly inferred to begin with)
//   - traversal_count == 0 (no one ever followed this edge)
//   - created_at older than 30 days (had its chance)
//
// Capped at 500 deletions per cycle so it stays under 30s and never
// monopolises DB locks. Logs every pass to `synapse_prune_log` for
// audit. Never touches edges with discovered_by IN ['user', 'manual']
// — only the auto-generated ones get pruned.

async fn circuit_synapse_prune(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    let cutoff = (chrono::Utc::now() - chrono::Duration::days(30)).to_rfc3339();

    // Step 1: count how many edges are eligible (for the log)
    let cutoff_c = cutoff.clone();
    let eligible: u64 = db.with_conn(move |conn| {
        let count: u64 = conn.query_row(
            "SELECT COUNT(*) FROM edges \
             WHERE strength < 0.3 \
               AND traversal_count = 0 \
               AND created_at < ?1 \
               AND discovered_by NOT IN ('user', 'manual')",
            params![cutoff_c],
            |row| row.get(0),
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(count)
    }).await?;

    if eligible == 0 {
        return Ok("No edges meet pruning criteria — graph is healthy".into());
    }

    // Step 2: select IDs to delete, then delete them (capped at 500)
    let cutoff_c = cutoff.clone();
    let deleted: u64 = db.with_conn(move |conn| {
        // Collect IDs first
        let mut stmt = conn.prepare(
            "SELECT id FROM edges \
             WHERE strength < 0.3 \
               AND traversal_count = 0 \
               AND created_at < ?1 \
               AND discovered_by NOT IN ('user', 'manual') \
             LIMIT 500"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let ids: Vec<String> = stmt.query_map(params![cutoff_c], |row| {
            row.get::<_, String>(0)
        }).map_err(|e| BrainError::Database(e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();

        let mut count = 0u64;
        for id in &ids {
            conn.execute("DELETE FROM edges WHERE id = ?1", params![id])
                .map_err(|e| BrainError::Database(e.to_string()))?;
            count += 1;
        }
        Ok(count)
    }).await?;

    // Step 3: log the pass
    let log_id = format!("synapse_prune_log:{}", uuid::Uuid::now_v7());
    let now = chrono::Utc::now().to_rfc3339();
    let reason = format!("Pruned {} weak synapses ({} eligible, cutoff {})", deleted, eligible, cutoff);
    let _ = db.with_conn(move |conn| {
        conn.execute(
            "INSERT INTO synapse_prune_log (id, pruned_count, reason, created_at) \
             VALUES (?1, ?2, ?3, ?4)",
            params![log_id, deleted, reason, now],
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(())
    }).await;

    Ok(format!(
        "Pruned {} weak synapses ({} eligible total)",
        deleted, eligible
    ))
}

// =========================================================================
// SHARED HELPERS — public so master_loop.rs and other modules can reuse
// =========================================================================

/// Truncate a string to `max` characters, appending an ellipsis if cut.
/// Pub so master_loop and any future modules can reuse it.
pub fn short(s: &str, max: usize) -> String {
    let s = s.trim().replace('\n', " ");
    if s.chars().count() <= max {
        s
    } else {
        let truncated: String = s.chars().take(max).collect();
        format!("{}…", truncated)
    }
}

// =========================================================================
// PERSISTENCE — circuit log + rotation
// =========================================================================

async fn log_circuit_run(
    db: &BrainDb,
    circuit_name: &str,
    started_at: &str,
    duration_ms: u64,
    status: &str,
    result: &str,
) -> Result<(), BrainError> {
    let log_id = format!("autonomy_circuit_log:{}", uuid::Uuid::now_v7());
    let name = circuit_name.to_string();
    let start = started_at.to_string();
    let st = status.to_string();
    let res = result.to_string();
    let _ = db.with_conn(move |conn| {
        conn.execute(
            "INSERT INTO autonomy_circuit_log (id, circuit_name, started_at, duration_ms, status, result) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![log_id, name, start, duration_ms, st, res],
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(())
    }).await;
    Ok(())
}

async fn load_recent_rotation(db: &BrainDb) -> Vec<String> {
    db.with_conn(|conn| {
        let result = conn.query_row(
            "SELECT recent_circuits FROM autonomy_circuit_rotation LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        );
        match result {
            Ok(json_str) => {
                let circuits: Vec<String> = serde_json::from_str(&json_str).unwrap_or_default();
                Ok(circuits)
            }
            Err(_) => Ok(Vec::new()),
        }
    }).await.unwrap_or_default()
}

async fn save_rotation(db: &BrainDb, recent: &[String]) -> Result<(), BrainError> {
    let now = chrono::Utc::now().to_rfc3339();
    let recent_json = serde_json::to_string(&recent).unwrap_or_else(|_| "[]".into());
    let rotation_id = "autonomy_circuit_rotation:singleton".to_string();
    db.with_conn(move |conn| {
        // Delete all existing rows then insert the singleton
        conn.execute("DELETE FROM autonomy_circuit_rotation", [])
            .map_err(|e| BrainError::Database(e.to_string()))?;
        conn.execute(
            "INSERT INTO autonomy_circuit_rotation (id, recent_circuits, updated_at) \
             VALUES (?1, ?2, ?3)",
            params![rotation_id, recent_json, now],
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(())
    }).await?;
    Ok(())
}

// =========================================================================
// HELPERS
// =========================================================================

fn cosine_sim(a: &[f64], b: &[f64]) -> f64 {
    if a.len() != b.len() || a.is_empty() { return 0.0; }
    let dot: f64 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let ma: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let mb: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();
    if ma == 0.0 || mb == 0.0 { return 0.0; }
    (dot / (ma * mb)).clamp(0.0, 1.0)
}

// =========================================================================
// CIRCUIT — fingerprint_synthesis (Phase Omega)
// =========================================================================
//
// Synthesizes the cognitive fingerprint from accumulated user data.
// Runs at lower frequency (every ~6 hours via rotation, since 20 circuits
// in the pool means each circuit runs roughly every 6.7 hours).

async fn circuit_fingerprint_synthesis(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    let fp = crate::cognitive_fingerprint::synthesize_fingerprint(db).await?;
    Ok(format!(
        "Fingerprint v{} synthesized (confidence={:.2}, {} domains, risk_tol={:.2}, decision_spd={:.2})",
        fp.version, fp.confidence, fp.expertise.len(), fp.risk_tolerance, fp.decision_speed
    ))
}

// =========================================================================
// CIRCUIT — internal_dialogue (Phase Omega)
// =========================================================================
//
// Picks a recent unresolved hypothesis or contradiction and runs a full
// advocate/critic/synthesizer dialogue to produce a verdict.

async fn circuit_internal_dialogue(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    crate::internal_dialogue::auto_dialogue(db).await
}

// =========================================================================
// CIRCUIT — temporal_analysis (Phase Omega III)
// =========================================================================
//
// Detects temporal patterns in node creation and validates past predictions.

async fn circuit_temporal_analysis(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    let patterns_result = crate::temporal::detect_temporal_patterns(db).await?;
    let validation_result = crate::temporal::validate_predictions(db).await?;
    Ok(format!("{}; {}", patterns_result, validation_result))
}

// =========================================================================
// CIRCUIT — causal_model_builder (Phase Omega III)
// =========================================================================
//
// Builds the causal world model from recent brain nodes using LLM extraction.

async fn circuit_causal_model_builder(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    let build_result = crate::world_model::build_causal_model(db).await?;
    let prediction_result = crate::temporal::generate_predictions(db).await?;
    Ok(format!("{}; {}", build_result, prediction_result))
}

// =========================================================================
// CIRCUIT — scenario_simulator (Phase Omega III)
// =========================================================================
//
// Picks a current trend from temporal patterns and simulates forward.

async fn circuit_scenario_simulator(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    // Pick a recent temporal pattern to simulate
    let trigger = db
        .with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT description FROM temporal_patterns \
                     WHERE confidence > 0.3 \
                     ORDER BY created_at DESC LIMIT 1",
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let result: Option<String> = stmt
                .query_row([], |row| row.get(0))
                .ok();
            Ok(result)
        })
        .await?;

    let trigger = match trigger {
        Some(t) => t,
        None => return Ok("No temporal patterns available for simulation".to_string()),
    };

    let prediction = crate::world_model::simulate_scenario(db, &trigger).await?;
    Ok(format!(
        "Simulated '{}': {} effects predicted (confidence {:.2})",
        crate::truncate_str(&trigger, 60),
        prediction.predicted_effects.len(),
        prediction.confidence
    ))
}

// =========================================================================
// CIRCUIT — knowledge_compiler (Phase Omega IV)
// =========================================================================
//
// Reads user cognition rules and decision nodes, uses LLM to extract
// structured, machine-parseable rules for instant lookup.

async fn circuit_knowledge_compiler(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    let result = crate::knowledge_compiler::compile_rules(db).await?;
    // Also validate existing rules
    let validation = crate::knowledge_compiler::validate_rules(db).await?;
    Ok(format!("{} | {}", result, validation))
}

// =========================================================================
// CIRCUIT — circuit_optimizer (Phase Omega IV)
// =========================================================================
//
// Computes circuit performance metrics and generates improvement suggestions
// for underperforming circuits.

async fn circuit_circuit_optimizer(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    let perfs = crate::circuit_performance::compute_circuit_performance(db).await?;
    let suggestions = crate::circuit_performance::suggest_improvements(db).await?;
    Ok(format!(
        "Analyzed {} circuits. Suggestions: {}",
        perfs.len(),
        crate::truncate_str(&suggestions, 200)
    ))
}

// =========================================================================
// CIRCUIT — capability_tracker (Phase Omega IV)
// =========================================================================
//
// Inventories the brain's knowledge capabilities and identifies gaps.

async fn circuit_capability_tracker(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    let caps = crate::capability_frontier::inventory_capabilities(db).await?;
    let gaps = crate::capability_frontier::identify_gaps(db).await?;
    let frontier = crate::capability_frontier::track_frontier(db).await?;
    Ok(format!(
        "{} capabilities tracked, {} gaps found. {}",
        caps.len(),
        gaps.len(),
        frontier
    ))
}

// =========================================================================
// CIRCUIT — self_reflection (Phase Omega IX — Consciousness Layer)
// =========================================================================
//
// Builds the self-model and generates a daily self-reflection insight node.

async fn circuit_self_reflection(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    crate::self_model::generate_self_reflection(db).await
}

// =========================================================================
// CIRCUIT — attention_update (Phase Omega IX — Consciousness Layer)
// =========================================================================
//
// Recomputes the attention window: scores all relevant nodes by the
// weighted attention formula and persists the top 100 focus nodes.

async fn circuit_attention_update(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    let window = crate::attention::compute_attention(db).await?;
    // Also track learning velocity while we're at it
    let velocities = crate::curiosity_v2::track_learning_velocity(db).await?;
    Ok(format!(
        "Attention updated: {} focus nodes, project={}, {} domain velocities tracked",
        window.focus_nodes.len(),
        window.current_project,
        velocities.len()
    ))
}

// =========================================================================
// CIRCUIT — curiosity_v2 (Phase Omega IX — Consciousness Layer)
// =========================================================================
//
// Runs the advanced curiosity engine: computes information gain for
// potential topics, picks strategic research targets + 10% serendipitous.

async fn circuit_curiosity_v2(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    let targets = crate::curiosity_v2::get_curiosity_targets(db, 10).await?;
    let strategic = targets.iter().filter(|t| !t.is_serendipity).count();
    let serendipitous = targets.iter().filter(|t| t.is_serendipity).count();
    let top_topic = targets.first().map(|t| t.topic.as_str()).unwrap_or("none");
    Ok(format!(
        "{} curiosity targets ({} strategic, {} serendipitous). Top: {}",
        targets.len(),
        strategic,
        serendipitous,
        top_topic
    ))
}

// =========================================================================
// CIRCUIT — cluster_health_check (Phase Omega VII — Infrastructure)
// =========================================================================
//
// Periodic cluster health check: marks stale nodes as offline, logs
// the overall cluster state, and checks system load for throttling.

async fn circuit_cluster_health_check(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    // 1. Check for stale nodes
    let stale = crate::distributed::check_nodes_health(db).await.unwrap_or(0);

    // 2. Get cluster overview
    let cluster = crate::distributed::get_cluster_status(db).await?;

    // 3. System health + throttle check
    let throttled = crate::system_health::should_throttle();
    let health_summary = match crate::system_health::get_system_health(db).await {
        Ok(h) => format!(
            "CPU ~{:.0}%, Mem {}/{}MB, DB {}MB, HNSW {}MB, queue {}, ollama {}",
            h.cpu_usage_percent,
            h.memory_used_mb,
            h.memory_total_mb,
            h.db_size_mb,
            h.hnsw_size_mb,
            h.embedding_queue_size,
            if h.ollama_available { "up" } else { "down" },
        ),
        Err(_) => "health unavailable".to_string(),
    };

    Ok(format!(
        "Cluster: {} nodes ({} online), {} marked offline. Throttle: {}. {}",
        cluster.total_nodes,
        cluster.online_count,
        stale,
        throttled,
        health_summary,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_circuit_avoiding_recent() {
        let recent = vec!["meta_reflection".to_string(), "user_pattern_mining".to_string()];
        let next = pick_next_circuit(&recent);
        assert!(!recent.iter().any(|r| r == next), "should not repeat last 3");
    }

    #[test]
    fn picks_first_when_empty() {
        let recent: Vec<String> = vec![];
        let next = pick_next_circuit(&recent);
        // Should pick from ALL_CIRCUITS, deterministic given empty rotation
        assert!(ALL_CIRCUITS.contains(&next));
    }

    #[test]
    fn rotates_through_all() {
        let mut recent: Vec<String> = vec![];
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        for _ in 0..ALL_CIRCUITS.len() * 2 {
            let pick = pick_next_circuit(&recent).to_string();
            seen.insert(pick.clone());
            recent.insert(0, pick);
            recent.truncate(ROTATION_WINDOW);
        }
        // After enough cycles, every circuit should have been picked
        assert_eq!(seen.len(), ALL_CIRCUITS.len());
    }

    #[test]
    fn similar_text_basic() {
        assert!(similar_text("user prefers tabs over spaces", "user prefers tabs over spaces") > 0.9);
        assert!(similar_text("apple banana", "orange mango") < 0.1);
    }
}
