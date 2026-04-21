use crate::ai::client::LlmClient;
use crate::ai::synthesize::BrainAnswer;
use crate::db::BrainDb;
use crate::embeddings::OllamaClient;
use crate::error::BrainError;
use rusqlite::params;
use std::sync::Arc;
use tauri::State;

/// LLM tier for routing — Phase 2.6
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum LlmTier {
    Default,
    Fast,
    Deep,
}

pub(crate) fn get_llm_client(db: &BrainDb) -> LlmClient {
    get_llm_client_for(db, LlmTier::Default)
}

#[allow(dead_code)]
pub(crate) fn get_llm_client_fast(db: &BrainDb) -> LlmClient {
    get_llm_client_for(db, LlmTier::Fast)
}

#[allow(dead_code)]
pub(crate) fn get_llm_client_deep(db: &BrainDb) -> LlmClient {
    get_llm_client_for(db, LlmTier::Deep)
}

/// CPU-routed LLM client for batch circuits. Falls back to the GPU pool
/// when `ollama_cpu_url` isn't configured, so circuits that ask for CPU
/// still work on machines with a single-daemon setup — they just don't
/// get the power savings. Phase 2.
#[allow(dead_code)]
pub(crate) fn get_llm_client_cpu(db: &BrainDb) -> LlmClient {
    get_llm_client_cpu_tier(db, LlmTier::Default)
}

#[allow(dead_code)]
pub(crate) fn get_llm_client_cpu_fast(db: &BrainDb) -> LlmClient {
    get_llm_client_cpu_tier(db, LlmTier::Fast)
}

fn get_llm_client_cpu_tier(db: &BrainDb, tier: LlmTier) -> LlmClient {
    // Delegate to the unified builder with force_cpu=true so we share the
    // config / fallback / telemetry logic.
    build_llm_client_for(db, tier, /* force_cpu */ true)
}

fn get_llm_client_for(db: &BrainDb, tier: LlmTier) -> LlmClient {
    build_llm_client_for(db, tier, /* force_cpu */ false)
}

fn build_llm_client_for(db: &BrainDb, tier: LlmTier, force_cpu: bool) -> LlmClient {
    let settings_path = db.config.data_dir.join("settings.json");
    let settings: Option<serde_json::Value> = if settings_path.exists() {
        std::fs::read_to_string(&settings_path)
            .ok()
            .and_then(|data| serde_json::from_str(&data).ok())
    } else {
        None
    };

    let get = |key: &str| -> Option<String> {
        settings
            .as_ref()
            .and_then(|s| s.get(key))
            .and_then(|v| v.as_str())
            .map(String::from)
    };

    let provider = get("llm_provider").unwrap_or_else(|| "ollama".to_string());
    let default_model = get("llm_model").unwrap_or_else(|| "qwen2.5-coder:14b".to_string());
    let model = match tier {
        LlmTier::Default => default_model.clone(),
        LlmTier::Fast => get("llm_model_fast").unwrap_or_else(|| default_model.clone()),
        LlmTier::Deep => get("llm_model_deep").unwrap_or_else(|| default_model.clone()),
    };
    let api_key = get("anthropic_api_key");

    // Phase 3: profile-based auto-routing. If the current circuit is Batch
    // (or caller explicitly asked for CPU) and a CPU daemon is configured,
    // route the call there — costs ~80W instead of ~300W. Falls back to
    // the GPU daemon when no CPU daemon is set so callers never break.
    //
    // Phase 4: adaptive policy can globally promote CPU routing — e.g.
    // Eco mode (on battery) demotes Interactive calls to CPU as well.
    let profile = crate::power_telemetry::current_profile();
    let policy_prefers_cpu = crate::power_policy::prefer_cpu();
    let should_cpu = force_cpu
        || policy_prefers_cpu
        || matches!(profile, crate::power_telemetry::CircuitProfile::Batch);

    if should_cpu {
        if let Some(cpu_url) = db.config.ollama_cpu_url.as_deref() {
            // Phase 5: CPU-specific model. A 14B+ model on CPU is ~1-3 tok/s,
            // defeating the purpose. Pick the user-configured `llm_model_cpu`,
            // or fall back to the fast-tier model, or a conservative default.
            let cpu_model = get("llm_model_cpu")
                .or_else(|| get("llm_model_fast"))
                .unwrap_or_else(|| "qwen2.5:3b".to_string());
            log::debug!(
                "Routing '{}' ({:?}) → ollama-cpu (model={})",
                crate::power_telemetry::current_circuit(),
                profile,
                cpu_model
            );
            return LlmClient::new("ollama", &cpu_model, cpu_url, None)
                .with_backend_tag("ollama-cpu");
        }
    }

    LlmClient::new(&provider, &model, &db.config.ollama_url, api_key)
}

#[tauri::command]
pub async fn ask_brain(
    db: State<'_, Arc<BrainDb>>,
    question: String,
) -> Result<BrainAnswer, BrainError> {
    let llm = get_llm_client(&db);
    let emb_client = OllamaClient::new(db.config.ollama_url.clone(), db.config.embedding_model.clone());

    let emb_ref = if emb_client.health_check().await { Some(&emb_client) } else { None };

    crate::ai::synthesize::answer_question(&db, &llm, emb_ref, &question).await
}

#[tauri::command]
pub async fn summarize_node_ai(
    db: State<'_, Arc<BrainDb>>,
    node_id: String,
) -> Result<String, BrainError> {
    let llm = get_llm_client(&db);
    crate::ai::summarize::summarize_node(&db, &llm, &node_id).await
}

#[tauri::command]
pub async fn backfill_summaries(
    db: State<'_, Arc<BrainDb>>,
) -> Result<(u64, u64), BrainError> {
    let llm = get_llm_client(&db);
    crate::ai::summarize::backfill_summaries(&db, &llm).await
}

#[tauri::command]
pub async fn extract_tags_ai(
    db: State<'_, Arc<BrainDb>>,
    node_id: String,
) -> Result<Vec<String>, BrainError> {
    let llm = get_llm_client(&db);

    let node_id_clone = node_id.clone();
    let node: crate::db::models::KnowledgeNode = db.with_conn(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, title, content, summary, content_hash, domain, topic, tags, node_type, \
             source_type, source_url, source_file, quality_score, visual_size, cluster_id, \
             created_at, updated_at, accessed_at, access_count, decay_score \
             FROM nodes WHERE id = ?1"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        stmt.query_row(params![node_id_clone], |row| {
            let tags_json: String = row.get(7)?;
            Ok(crate::db::models::KnowledgeNode {
                id: row.get(0)?,
                title: row.get(1)?,
                content: row.get(2)?,
                summary: row.get(3)?,
                content_hash: row.get(4)?,
                domain: row.get(5)?,
                topic: row.get(6)?,
                tags: serde_json::from_str(&tags_json).unwrap_or_default(),
                node_type: row.get(8)?,
                source_type: row.get(9)?,
                source_url: row.get(10)?,
                source_file: row.get(11)?,
                quality_score: row.get(12)?,
                visual_size: row.get(13)?,
                cluster_id: row.get(14)?,
                created_at: row.get(15)?,
                updated_at: row.get(16)?,
                accessed_at: row.get(17)?,
                access_count: row.get(18)?,
                decay_score: row.get(19)?,
                embedding: None,
                synthesized_by_brain: false,
                cognitive_type: None,
                confidence: None,
                memory_tier: None,
                compression_parent: None,
                brain_id: None,
            })
        }).map_err(|e| BrainError::Database(e.to_string()))
    }).await?;

    let tags = llm.extract_tags(&node.content).await?;

    // Update node tags (merge with existing)
    let mut merged = node.tags.clone();
    for tag in &tags {
        if !merged.contains(tag) {
            merged.push(tag.clone());
        }
    }
    let tags_json = serde_json::to_string(&merged).unwrap_or_else(|_| "[]".to_string());
    let id = node_id.clone();
    db.with_conn(move |conn| {
        conn.execute(
            "UPDATE nodes SET tags = ?1 WHERE id = ?2",
            params![tags_json, id],
        ).map_err(|e| BrainError::Database(e.to_string()))
    }).await?;

    Ok(tags)
}

// =========================================================================
// Phase Omega — Cognitive Fingerprint
// =========================================================================

#[tauri::command]
pub async fn get_cognitive_fingerprint(
    db: State<'_, Arc<BrainDb>>,
) -> Result<Option<crate::cognitive_fingerprint::CognitiveFingerprint>, BrainError> {
    crate::cognitive_fingerprint::get_fingerprint(&db).await
}

#[tauri::command]
pub async fn synthesize_cognitive_fingerprint(
    db: State<'_, Arc<BrainDb>>,
) -> Result<crate::cognitive_fingerprint::CognitiveFingerprint, BrainError> {
    crate::cognitive_fingerprint::synthesize_fingerprint(&db).await
}

// =========================================================================
// Phase Omega — Decision Simulator
// =========================================================================

#[tauri::command]
pub async fn simulate_decision(
    db: State<'_, Arc<BrainDb>>,
    question: String,
) -> Result<crate::decision_simulator::SimulatedDecision, BrainError> {
    crate::decision_simulator::simulate_decision(&db, &question).await
}

// =========================================================================
// Phase Omega — Internal Dialogue
// =========================================================================

#[tauri::command]
pub async fn run_dialogue(
    db: State<'_, Arc<BrainDb>>,
    topic: String,
) -> Result<crate::internal_dialogue::Dialogue, BrainError> {
    crate::internal_dialogue::run_dialogue(&db, &topic).await
}
