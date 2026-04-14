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

fn get_llm_client_for(db: &BrainDb, tier: LlmTier) -> LlmClient {
    let settings_path = db.config.data_dir.join("settings.json");
    let (provider, model, api_key) = if settings_path.exists() {
        if let Ok(data) = std::fs::read_to_string(&settings_path) {
            if let Ok(s) = serde_json::from_str::<serde_json::Value>(&data) {
                let provider = s.get("llm_provider").and_then(|v| v.as_str()).unwrap_or("ollama").to_string();
                let default_model = s.get("llm_model").and_then(|v| v.as_str()).unwrap_or("qwen2.5-coder:14b").to_string();
                let model = match tier {
                    LlmTier::Default => default_model.clone(),
                    LlmTier::Fast => s.get("llm_model_fast")
                        .and_then(|v| v.as_str())
                        .unwrap_or(&default_model)
                        .to_string(),
                    LlmTier::Deep => s.get("llm_model_deep")
                        .and_then(|v| v.as_str())
                        .unwrap_or(&default_model)
                        .to_string(),
                };
                let api_key = s.get("anthropic_api_key").and_then(|v| v.as_str()).map(String::from);
                (provider, model, api_key)
            } else {
                ("ollama".to_string(), "qwen2.5-coder:14b".to_string(), None)
            }
        } else {
            ("ollama".to_string(), "qwen2.5-coder:14b".to_string(), None)
        }
    } else {
        let model = match tier {
            LlmTier::Deep => "qwen2.5-coder:32b",
            _ => "qwen2.5-coder:14b",
        }.to_string();
        ("ollama".to_string(), model, None)
    };

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
