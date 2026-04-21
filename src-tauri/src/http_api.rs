//! HTTP API — the brain's external interface for the standalone MCP server.

use crate::db::models::{SearchResult, UserCognition};
use crate::db::BrainDb;
use sha2::Digest;
use axum::{extract::State, http::StatusCode, response::IntoResponse, routing::{get, post}, Json, Router};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};

#[derive(Clone)]
pub struct AppState { pub db: Arc<BrainDb> }

pub async fn run_http_server(db: Arc<BrainDb>) {
    let port = db.config.http_api_port();
    let addr: std::net::SocketAddr = ([127, 0, 0, 1], port).into();
    let state = AppState { db };
    let cors = CorsLayer::new().allow_origin(Any).allow_methods(Any).allow_headers(Any);
    let app = Router::new()
        .route("/health", get(handle_health))
        .route("/stats", get(handle_stats))
        .route("/metrics/power", get(handle_metrics_power))
        .route("/metrics/power/status", get(handle_metrics_power_status))
        .route("/brain/recall", post(handle_recall))
        .route("/brain/context", post(handle_context))
        .route("/brain/preferences", post(handle_preferences))
        .route("/brain/decisions", post(handle_decisions))
        .route("/brain/learn", post(handle_learn))
        .route("/repair/scan_nodes", post(handle_scan_nodes))
        .route("/repair/scan_edges", post(handle_scan_edges))
        .route("/repair/delete", post(handle_repair_delete))
        .route("/compact/export", post(handle_compact_export))
        .route("/compact/import", post(handle_compact_import))
        .route("/brain/critique", post(handle_critique))
        .route("/brain/history", post(handle_history))
        .route("/brain/export_subgraph", post(handle_export_subgraph))
        .route("/brain/plan", post(handle_plan))
        .route("/import/markdown_nodes", post(handle_import_markdown_nodes))
        // Phase Omega
        .route("/brain/simulate", post(handle_simulate_decision))
        .route("/brain/dialogue", post(handle_dialogue))
        .route("/brain/fingerprint", get(handle_get_fingerprint))
        .route("/brain/fingerprint/synthesize", post(handle_synthesize_fingerprint))
        // Phase Omega II — Swarm
        .route("/swarm/status", get(handle_swarm_status))
        .route("/swarm/task", post(handle_swarm_create_task))
        .route("/swarm/goal", post(handle_swarm_decompose_goal))
        // Phase Omega III — World Model
        .route("/world/entities", get(handle_world_entities))
        .route("/world/links", get(handle_world_links))
        .route("/world/simulate", post(handle_world_simulate))
        .route("/world/predictions", get(handle_world_predictions))
        // Phase Omega IV — Recursive Self-Improvement
        .route("/self/rules", get(handle_self_rules))
        .route("/self/performance", get(handle_self_performance))
        .route("/self/capabilities", get(handle_self_capabilities))
        .route("/self/compile", post(handle_self_compile))
        // Phase Omega IX — Consciousness Layer
        .route("/consciousness/self", get(handle_consciousness_self))
        .route("/consciousness/attention", get(handle_consciousness_attention))
        .route("/consciousness/curiosity", get(handle_consciousness_curiosity))
        // Phase Omega VII — Infrastructure
        .route("/infra/cluster", get(handle_infra_cluster))
        .route("/infra/node", post(handle_infra_register_node))
        .route("/infra/health", get(handle_infra_health))
        .route("/infra/edge_devices", get(handle_infra_edge_devices))
        // Phase Omega V — Sensory Expansion
        .route("/sensory/analyze_image", post(handle_sensory_analyze_image))
        .route("/sensory/transcribe", post(handle_sensory_transcribe))
        .route("/sensory/streams", get(handle_sensory_get_streams))
        .route("/sensory/streams/add", post(handle_sensory_add_stream))
        .route("/sensory/streams/poll", post(handle_sensory_poll_streams))
        // Phase Omega VI — Federation (The Collective)
        .route("/federation/status", get(handle_federation_status))
        .route("/federation/register", post(handle_federation_register))
        .route("/federation/share", post(handle_federation_share))
        .route("/federation/sync", post(handle_federation_sync))
        .route("/federation/receive", post(handle_federation_receive))
        // Phase Omega VIII — Economic Autonomy
        .route("/economics/revenue", post(handle_economics_revenue))
        .route("/economics/cost", post(handle_economics_cost))
        .route("/economics/report", get(handle_economics_report))
        .route("/economics/sustaining", get(handle_economics_sustaining))
        // Dual-Brain — Context Bundle + Learning Tools (Phase 1 + 2)
        .route("/brain/bundle", post(handle_context_bundle))
        .route("/brain/warnings", post(handle_brain_warnings))
        .route("/brain/rules", post(handle_brain_rules))
        .route("/brain/learn_decision", post(handle_learn_decision))
        .route("/brain/learn_pattern", post(handle_learn_pattern))
        .route("/brain/learn_mistake", post(handle_learn_mistake))
        .route("/brain/session_handoff", get(handle_session_handoff))
        .layer(cors).with_state(state);

    log::info!("HTTP API: binding to http://{}", addr);
    let listener = match tokio::net::TcpListener::bind(addr).await { Ok(l) => l, Err(e) => { log::error!("HTTP API: failed to bind — {}", e); return; } };
    log::info!("HTTP API: listening on http://{}", addr);
    if let Err(e) = axum::serve(listener, app).await { log::error!("HTTP API: server error: {}", e); }
}

#[derive(Serialize)]
struct ApiError { error: String }
fn err(status: StatusCode, msg: impl Into<String>) -> (StatusCode, Json<ApiError>) { (status, Json(ApiError { error: msg.into() })) }

async fn handle_health() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok", "service": "neurovault", "version": env!("CARGO_PKG_VERSION") }))
}

async fn handle_stats(State(state): State<AppState>) -> impl IntoResponse {
    match state.db.get_brain_stats().await { Ok(stats) => Json(serde_json::to_value(&stats).unwrap_or(serde_json::Value::Null)).into_response(), Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response() }
}

/// Power telemetry rollup. Accepts `?hours=N` (default 24, clamped to [1, 720]).
#[derive(Deserialize)]
struct PowerQuery { hours: Option<i64> }

async fn handle_metrics_power(
    State(state): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<PowerQuery>,
) -> impl IntoResponse {
    let hours = q.hours.unwrap_or(24).clamp(1, 24 * 30);
    match crate::power_telemetry::rollup_power(&state.db, hours).await {
        Ok(summary) => Json(serde_json::to_value(&summary).unwrap_or(serde_json::Value::Null)).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// Live policy snapshot — current PowerMode, whether CPU routing is
/// active, backend wattage coefficients. Used by the dashboard to
/// show what the brain is actually doing right now.
async fn handle_metrics_power_status(State(state): State<AppState>) -> impl IntoResponse {
    let status = crate::power_telemetry::power_status(&state.db);
    Json(serde_json::to_value(&status).unwrap_or(serde_json::Value::Null)).into_response()
}

#[derive(Deserialize)] struct RecallReq { query: String, limit: Option<usize> }
#[derive(Serialize)] struct RecallResp { query: String, matches: Vec<SearchResult>, source_count: usize }

async fn handle_recall(State(state): State<AppState>, Json(req): Json<RecallReq>) -> impl IntoResponse {
    let limit = req.limit.unwrap_or(10);
    let _ = log_call(&state.db, "brain_recall", &req.query).await;
    let client = crate::embeddings::OllamaClient::new(state.db.config.ollama_url.clone(), state.db.config.embedding_model.clone());
    let matches = if client.health_check().await {
        match client.generate_embedding(&req.query).await { Ok(emb) => state.db.vector_search(emb, limit).await.unwrap_or_default(), Err(_) => state.db.search_nodes(&req.query).await.unwrap_or_default() }
    } else { state.db.search_nodes(&req.query).await.unwrap_or_default() };
    let source_count = matches.len();
    Json(RecallResp { query: req.query, matches, source_count }).into_response()
}

#[derive(Deserialize)] struct ContextReq { file_path: String }
#[derive(Serialize)] struct ContextResp { file_path: String, matches: Vec<SearchResult>, source_count: usize }

async fn handle_context(State(state): State<AppState>, Json(req): Json<ContextReq>) -> impl IntoResponse {
    let _ = log_call(&state.db, "brain_context", &req.file_path).await;
    let path = std::path::Path::new(&req.file_path);
    let filename = path.file_name().and_then(|s| s.to_str()).unwrap_or(&req.file_path).to_string();
    let parent = path.parent().and_then(|p| p.file_name()).and_then(|s| s.to_str()).unwrap_or("");
    let query = if parent.is_empty() { filename } else { format!("{} {}", parent, filename) };
    let matches = state.db.search_nodes(&query).await.unwrap_or_default();
    let source_count = matches.len();
    Json(ContextResp { file_path: req.file_path, matches, source_count }).into_response()
}

#[derive(Deserialize)] struct PreferencesReq { pattern_type: Option<String> }
#[derive(Serialize)] struct PreferencesResp { rules: Vec<UserCognition>, total_count: usize }

async fn handle_preferences(State(state): State<AppState>, Json(req): Json<PreferencesReq>) -> impl IntoResponse {
    let _ = log_call(&state.db, "brain_preferences", req.pattern_type.as_deref().unwrap_or("ALL")).await;
    let pt = req.pattern_type.clone();
    let rules: Result<Vec<UserCognition>, _> = state.db.with_conn(move |conn| {
        let mut result = Vec::new();
        if let Some(pt) = &pt {
            let mut stmt = conn.prepare("SELECT id, timestamp, trigger_node_ids, pattern_type, extracted_rule, structured_rule, confidence, times_confirmed, times_contradicted, embedding, linked_to_nodes FROM user_cognition WHERE pattern_type = ?1 LIMIT 100")
                .map_err(|e| crate::error::BrainError::Database(e.to_string()))?;
            let rows = stmt.query_map(params![pt], |row| Ok(UserCognition { id: row.get(0)?, timestamp: row.get(1)?, trigger_node_ids: serde_json::from_str(&row.get::<_, String>(2)?).unwrap_or_default(), pattern_type: row.get(3)?, extracted_rule: row.get(4)?, structured_rule: row.get(5)?, confidence: row.get(6)?, times_confirmed: row.get(7)?, times_contradicted: row.get(8)?, embedding: None, linked_to_nodes: serde_json::from_str(&row.get::<_, String>(10)?).unwrap_or_default() }))
                .map_err(|e| crate::error::BrainError::Database(e.to_string()))?;
            for r in rows { if let Ok(c) = r { result.push(c); } }
        } else {
            let mut stmt = conn.prepare("SELECT id, timestamp, trigger_node_ids, pattern_type, extracted_rule, structured_rule, confidence, times_confirmed, times_contradicted, embedding, linked_to_nodes FROM user_cognition LIMIT 200")
                .map_err(|e| crate::error::BrainError::Database(e.to_string()))?;
            let rows = stmt.query_map([], |row| Ok(UserCognition { id: row.get(0)?, timestamp: row.get(1)?, trigger_node_ids: serde_json::from_str(&row.get::<_, String>(2)?).unwrap_or_default(), pattern_type: row.get(3)?, extracted_rule: row.get(4)?, structured_rule: row.get(5)?, confidence: row.get(6)?, times_confirmed: row.get(7)?, times_contradicted: row.get(8)?, embedding: None, linked_to_nodes: serde_json::from_str(&row.get::<_, String>(10)?).unwrap_or_default() }))
                .map_err(|e| crate::error::BrainError::Database(e.to_string()))?;
            for r in rows { if let Ok(c) = r { result.push(c); } }
        }
        Ok(result)
    }).await;
    match rules {
        Ok(mut r) => { r.sort_by(|a, b| { let sa = a.confidence * (a.times_confirmed as f32 + 1.0); let sb = b.confidence * (b.times_confirmed as f32 + 1.0); sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal) }); let total_count = r.len(); Json(PreferencesResp { rules: r, total_count }).into_response() }
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Deserialize)] struct DecisionsReq { topic: String }
#[derive(Serialize)] struct DecisionsResp { topic: String, decisions: Vec<SearchResult> }

async fn handle_decisions(State(state): State<AppState>, Json(req): Json<DecisionsReq>) -> impl IntoResponse {
    let _ = log_call(&state.db, "brain_decisions", &req.topic).await;
    let all = state.db.search_nodes(&req.topic).await.unwrap_or_default();
    let decisions: Vec<SearchResult> = all.into_iter().filter(|m| m.node.node_type == crate::db::models::NODE_TYPE_DECISION || m.node.tags.iter().any(|t| t == "decision")).take(10).collect();
    Json(DecisionsResp { topic: req.topic, decisions }).into_response()
}

#[derive(Deserialize)] struct LearnReq { observation: String, pattern_type: Option<String>, trigger_node_id: Option<String> }
#[derive(Serialize)] struct LearnResp { stored_id: String, action: String }

async fn handle_learn(State(state): State<AppState>, Json(req): Json<LearnReq>) -> impl IntoResponse {
    let _ = log_call(&state.db, "brain_learn", &req.observation).await;
    let pattern_type = req.pattern_type.unwrap_or_else(|| "general".to_string()).to_lowercase();
    let trigger_ids = req.trigger_node_id.map(|id| vec![id]).unwrap_or_default();
    let now = chrono::Utc::now().to_rfc3339();
    let id = format!("user_cognition:{}", uuid::Uuid::now_v7());
    let stored_id = id.clone();
    let trigger_json = serde_json::to_string(&trigger_ids).unwrap_or_else(|_| "[]".to_string());
    let rule = req.observation.clone();
    let _ = state.db.with_conn(move |conn| {
        conn.execute("INSERT INTO user_cognition (id, timestamp, trigger_node_ids, pattern_type, extracted_rule, structured_rule, confidence, times_confirmed, times_contradicted, embedding, linked_to_nodes) VALUES (?1, ?2, ?3, ?4, ?5, NULL, 0.7, 1, 0, NULL, '[]')", params![id, now, trigger_json, pattern_type, rule])
            .map_err(|e| crate::error::BrainError::Database(e.to_string()))
    }).await;
    Json(LearnResp { stored_id, action: "created".into() }).into_response()
}

async fn handle_compact_export(State(state): State<AppState>) -> impl IntoResponse {
    match crate::commands::compact::compact_export_all_inner(&state.db).await { Ok(r) => Json(serde_json::to_value(&r).unwrap_or_default()).into_response(), Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response() }
}
async fn handle_compact_import(State(state): State<AppState>) -> impl IntoResponse {
    match crate::commands::compact::compact_import_all_inner(&state.db).await { Ok(r) => Json(serde_json::to_value(&r).unwrap_or_default()).into_response(), Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response() }
}
async fn handle_scan_nodes(State(state): State<AppState>) -> impl IntoResponse {
    match crate::commands::repair::scan_table_inner(&state.db, "nodes").await { Ok(r) => Json(serde_json::to_value(&r).unwrap_or_default()).into_response(), Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response() }
}
async fn handle_scan_edges(State(state): State<AppState>) -> impl IntoResponse {
    match crate::commands::repair::scan_table_inner(&state.db, "edges").await { Ok(r) => Json(serde_json::to_value(&r).unwrap_or_default()).into_response(), Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response() }
}

#[derive(Deserialize)] struct RepairDeleteReq { records: Vec<crate::commands::repair::CorruptedRecord> }
async fn handle_repair_delete(State(state): State<AppState>, Json(req): Json<RepairDeleteReq>) -> impl IntoResponse {
    match crate::commands::repair::delete_corrupted_inner(&state.db, req.records).await { Ok(d) => Json(serde_json::json!({"deleted": d})).into_response(), Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response() }
}

#[derive(Deserialize)] struct CritiqueReq { text: String }
async fn handle_critique(State(state): State<AppState>, Json(req): Json<CritiqueReq>) -> impl IntoResponse {
    let _ = log_call(&state.db, "brain_critique", &req.text).await;
    let rules: Vec<UserCognition> = state.db.with_conn(|conn| {
        let mut stmt = conn.prepare("SELECT id, timestamp, trigger_node_ids, pattern_type, extracted_rule, structured_rule, confidence, times_confirmed, times_contradicted, embedding, linked_to_nodes FROM user_cognition WHERE confidence > 0.5 LIMIT 200").map_err(|e| crate::error::BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| Ok(UserCognition { id: row.get(0)?, timestamp: row.get(1)?, trigger_node_ids: serde_json::from_str(&row.get::<_, String>(2)?).unwrap_or_default(), pattern_type: row.get(3)?, extracted_rule: row.get(4)?, structured_rule: row.get(5)?, confidence: row.get(6)?, times_confirmed: row.get(7)?, times_contradicted: row.get(8)?, embedding: None, linked_to_nodes: serde_json::from_str(&row.get::<_, String>(10)?).unwrap_or_default() })).map_err(|e| crate::error::BrainError::Database(e.to_string()))?;
        let mut result = Vec::new(); for r in rows { if let Ok(c) = r { result.push(c); } } Ok(result)
    }).await.unwrap_or_default();
    let text_lower = req.text.to_lowercase();
    let text_words: std::collections::HashSet<String> = text_lower.split_whitespace().filter(|w| w.len() > 4).map(|s| s.trim_matches(|c: char| !c.is_alphanumeric()).to_string()).filter(|s| !s.is_empty()).collect();
    let mut matches = Vec::new(); let mut conflicts = Vec::new();
    for rule in rules {
        let rl = rule.extracted_rule.to_lowercase();
        let rw: std::collections::HashSet<String> = rl.split_whitespace().filter(|w| w.len() > 4).map(|s| s.trim_matches(|c: char| !c.is_alphanumeric()).to_string()).filter(|s| !s.is_empty()).collect();
        if text_words.intersection(&rw).count() >= 2 {
            if rl.contains("don't") || rl.contains("never") || rl.contains("avoid") || rl.contains("not ") { conflicts.push(rule); } else { matches.push(rule); }
        }
    }
    matches.truncate(8); conflicts.truncate(8);
    Json(serde_json::json!({ "text": req.text, "matches_user_patterns": matches, "conflicts_with_user_patterns": conflicts, "summary": format!("Found {} aligned patterns and {} potential conflicts", matches.len(), conflicts.len()) })).into_response()
}

#[derive(Deserialize)] struct HistoryReq { topic: String }
async fn handle_history(State(state): State<AppState>, Json(req): Json<HistoryReq>) -> impl IntoResponse {
    let _ = log_call(&state.db, "brain_history", &req.topic).await;
    let topic = req.topic.clone();
    let mut rows: Vec<serde_json::Value> = state.db.with_conn(move |conn| {
        let mut stmt = conn.prepare("SELECT title, node_type, source_type, summary, created_at FROM nodes WHERE topic = ?1 ORDER BY created_at ASC LIMIT 50").map_err(|e| crate::error::BrainError::Database(e.to_string()))?;
        let r = stmt.query_map(params![topic], |row| Ok(serde_json::json!({ "title": row.get::<_, String>(0)?, "node_type": row.get::<_, String>(1)?, "source_type": row.get::<_, String>(2)?, "summary": row.get::<_, String>(3)?, "created_at": row.get::<_, String>(4)? }))).map_err(|e| crate::error::BrainError::Database(e.to_string()))?;
        let mut result = Vec::new(); for row in r { if let Ok(v) = row { result.push(v); } } Ok(result)
    }).await.unwrap_or_default();
    if rows.is_empty() {
        let sr = state.db.search_nodes(&req.topic).await.unwrap_or_default();
        for s in sr.into_iter().take(50) { rows.push(serde_json::json!({ "title": s.node.title, "node_type": s.node.node_type, "source_type": s.node.source_type, "summary": s.node.summary, "created_at": s.node.created_at })); }
        rows.sort_by(|a, b| a.get("created_at").and_then(|v| v.as_str()).unwrap_or("").cmp(b.get("created_at").and_then(|v| v.as_str()).unwrap_or("")));
    }
    let sc = rows.len();
    Json(serde_json::json!({ "topic": req.topic, "timeline": rows, "source_count": sc })).into_response()
}

#[derive(Deserialize)] struct ExportSubgraphReq { node_ids: Vec<String> }
async fn handle_export_subgraph(State(state): State<AppState>, Json(req): Json<ExportSubgraphReq>) -> impl IntoResponse {
    let _ = log_call(&state.db, "brain_export_subgraph", &req.node_ids.join(",")).await;
    let mut all_ids: std::collections::HashSet<String> = req.node_ids.iter().cloned().collect();
    let mut edges_out = Vec::new();
    for nid in &req.node_ids {
        let neighs = state.db.get_edges_for_node(nid).await.unwrap_or_default();
        for e in neighs { all_ids.insert(e.source.clone()); all_ids.insert(e.target.clone()); edges_out.push(e); }
    }
    let ids_vec: Vec<String> = all_ids.into_iter().collect();
    let nodes_out: Vec<crate::db::models::GraphNode> = state.db.with_conn(move |conn| {
        let mut result = Vec::new();
        for id in &ids_vec {
            let mut stmt = conn.prepare("SELECT id, title, content, summary, domain, topic, tags, node_type, source_type, visual_size, access_count, decay_score, created_at FROM nodes WHERE id = ?1").map_err(|e| crate::error::BrainError::Database(e.to_string()))?;
            if let Ok(n) = stmt.query_row(params![id], |row| { let tj: String = row.get(6)?; Ok(crate::db::models::GraphNode { id: row.get(0)?, title: row.get(1)?, content: row.get(2)?, summary: row.get(3)?, domain: row.get(4)?, topic: row.get(5)?, tags: serde_json::from_str(&tj).unwrap_or_default(), node_type: row.get(7)?, source_type: row.get(8)?, visual_size: row.get(9)?, access_count: row.get(10)?, decay_score: row.get(11)?, created_at: row.get(12)? }) }) { result.push(n); }
        }
        Ok(result)
    }).await.unwrap_or_default();
    let sc = nodes_out.len();
    Json(serde_json::json!({ "node_ids": req.node_ids, "nodes": nodes_out, "edges": edges_out, "source_count": sc })).into_response()
}

#[derive(Deserialize)] struct PlanReq { task: String }
async fn handle_plan(State(state): State<AppState>, Json(req): Json<PlanReq>) -> impl IntoResponse {
    let _ = log_call(&state.db, "brain_plan", &req.task).await;
    let client = crate::embeddings::OllamaClient::new(state.db.config.ollama_url.clone(), state.db.config.embedding_model.clone());
    let related = if client.health_check().await { match client.generate_embedding(&req.task).await { Ok(emb) => state.db.vector_search(emb, 5).await.unwrap_or_default(), Err(_) => state.db.search_nodes(&req.task).await.unwrap_or_default() } } else { state.db.search_nodes(&req.task).await.unwrap_or_default() };
    let rules: Vec<UserCognition> = state.db.with_conn(|conn| {
        let mut stmt = conn.prepare("SELECT id, timestamp, trigger_node_ids, pattern_type, extracted_rule, structured_rule, confidence, times_confirmed, times_contradicted, embedding, linked_to_nodes FROM user_cognition WHERE confidence > 0.6 ORDER BY confidence DESC LIMIT 10").map_err(|e| crate::error::BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| Ok(UserCognition { id: row.get(0)?, timestamp: row.get(1)?, trigger_node_ids: serde_json::from_str(&row.get::<_, String>(2)?).unwrap_or_default(), pattern_type: row.get(3)?, extracted_rule: row.get(4)?, structured_rule: row.get(5)?, confidence: row.get(6)?, times_confirmed: row.get(7)?, times_contradicted: row.get(8)?, embedding: None, linked_to_nodes: serde_json::from_str(&row.get::<_, String>(10)?).unwrap_or_default() })).map_err(|e| crate::error::BrainError::Database(e.to_string()))?;
        let mut result = Vec::new(); for r in rows { if let Ok(c) = r { result.push(c); } } Ok(result)
    }).await.unwrap_or_default();
    let mut ctx = String::new();
    if !related.is_empty() { ctx.push_str("RELEVANT PAST KNOWLEDGE:\n"); for r in related.iter().take(5) { ctx.push_str(&format!("- {}: {}\n", r.node.title, r.node.summary)); } ctx.push('\n'); }
    if !rules.is_empty() { ctx.push_str("USER'S ESTABLISHED PREFERENCES:\n"); for r in rules.iter().take(5) { ctx.push_str(&format!("- ({}) {}\n", r.pattern_type, r.extracted_rule)); } ctx.push('\n'); }
    let llm = crate::commands::ai::get_llm_client_fast(&state.db);
    let prompt = format!("You are NeuroVault's planner. Generate a concrete step-by-step plan for the task below, following the user's established preferences and reusing relevant past decisions. Output as a numbered list, 4-8 steps. No preamble.\n\n{}\n\nTASK: {}", ctx, req.task);
    let plan = match llm.generate(&prompt, 600).await { Ok(p) => p, Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response() };
    let used_nodes: Vec<String> = related.iter().take(5).map(|r| r.node.title.clone()).collect();
    let used_prefs: Vec<String> = rules.iter().take(5).map(|r| r.extracted_rule.clone()).collect();
    Json(serde_json::json!({ "task": req.task, "plan": plan, "used_nodes": used_nodes, "used_preferences": used_prefs })).into_response()
}

async fn log_call(db: &BrainDb, command: &str, payload: &str) {
    let now = chrono::Utc::now().to_rfc3339();
    let payload_trunc = if payload.len() > 500 { format!("{}...", &payload[..500]) } else { payload.to_string() };
    let id = format!("mcp_call_log:{}", uuid::Uuid::now_v7());
    let cmd = command.to_string();
    let _ = db.with_conn(move |conn| {
        conn.execute("INSERT INTO mcp_call_log (id, tool_name, args, result, called_at) VALUES (?1, ?2, ?3, '', ?4)", params![id, cmd, payload_trunc, now])
            .map_err(|e| crate::error::BrainError::Database(e.to_string()))
    }).await;
}

/// Import markdown node files from the old export directory into SQLite.
async fn handle_import_markdown_nodes(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let db = &state.db;
    let nodes_dir = db.config.export_dir().join("nodes");

    if !nodes_dir.exists() {
        return Json(serde_json::json!({ "error": "No nodes directory found", "path": nodes_dir.to_string_lossy() }));
    }

    let db_clone = db.clone();
    let result = tokio::task::spawn_blocking(move || {
        let mut imported = 0u64;
        let mut skipped = 0u64;
        let mut errors = 0u64;

        // Walk all domain subdirectories
        for domain_entry in std::fs::read_dir(&nodes_dir).into_iter().flatten().flatten() {
            let domain_path = domain_entry.path();
            if !domain_path.is_dir() { continue; }
            let domain = domain_path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("general")
                .to_string();

            for file_entry in std::fs::read_dir(&domain_path).into_iter().flatten().flatten() {
                let file_path = file_entry.path();
                if file_path.extension().map(|e| e != "md").unwrap_or(true) { continue; }

                let content = match std::fs::read_to_string(&file_path) {
                    Ok(c) => c,
                    Err(_) => { errors += 1; continue; }
                };

                // Parse the markdown format
                let mut title = file_path.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Untitled")
                    .to_string();
                let mut topic = String::new();
                let mut tags: Vec<String> = Vec::new();
                let mut node_type = "reference".to_string();
                let mut body = content.clone();

                // Extract metadata from header line: **Domain:** x | **Topic:** y | **Type:** z
                for line in content.lines().take(10) {
                    if line.starts_with("# ") {
                        title = line[2..].trim().to_string();
                    }
                    if line.contains("**Topic:**") {
                        if let Some(t) = line.split("**Topic:**").nth(1) {
                            topic = t.split('|').next().unwrap_or("").trim().to_string();
                        }
                    }
                    if line.contains("**Type:**") {
                        if let Some(t) = line.split("**Type:**").nth(1) {
                            node_type = t.trim().to_string();
                        }
                    }
                    if line.contains("**Tags:**") {
                        if let Some(t) = line.split("**Tags:**").nth(1) {
                            tags = t.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                        }
                    }
                }

                // Get body after the --- separator
                if let Some(sep_pos) = content.find("\n---\n") {
                    body = content[sep_pos + 5..].trim().to_string();
                }

                if body.is_empty() { skipped += 1; continue; }

                // Compute content hash for dedup
                use sha2::{Digest, Sha256};
                let content_hash = format!("{:x}", Sha256::digest(body.as_bytes()));

                let conn = db_clone.conn_raw();
                let conn = conn.lock().unwrap();

                // Check if content already exists
                let exists: bool = conn.query_row(
                    "SELECT COUNT(*) FROM nodes WHERE content_hash = ?1",
                    rusqlite::params![content_hash],
                    |row| row.get::<_, u64>(0),
                ).unwrap_or(0) > 0;

                if exists { skipped += 1; continue; }

                let id = format!("node:{}", uuid::Uuid::now_v7());
                let now = chrono::Utc::now().to_rfc3339();
                let summary = if body.len() > 200 {
                    format!("{}...", &body[..body.char_indices().take_while(|&(i, _)| i < 200).last().map(|(i, c)| i + c.len_utf8()).unwrap_or(200)])
                } else {
                    body.clone()
                };
                let tags_json = serde_json::to_string(&tags).unwrap_or_else(|_| "[]".to_string());

                match conn.execute(
                    "INSERT INTO nodes (id, title, content, summary, content_hash, domain, topic, tags,
                                        node_type, source_type, quality_score, visual_size,
                                        decay_score, access_count, synthesized_by_brain,
                                        created_at, updated_at, accessed_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'import', 0.7, 3.0, 1.0, 0, 0, ?10, ?10, ?10)",
                    rusqlite::params![id, title, body, summary, content_hash, domain, topic, tags_json, node_type, now],
                ) {
                    Ok(_) => imported += 1,
                    Err(_) => { skipped += 1; }
                }

                if imported % 1000 == 0 && imported > 0 {
                    log::info!("Import progress: {} imported, {} skipped", imported, skipped);
                }
            }
        }

        (imported, skipped, errors)
    }).await;

    match result {
        Ok((imported, skipped, errors)) => {
            log::info!("Markdown import complete: {} imported, {} skipped, {} errors", imported, skipped, errors);
            Json(serde_json::json!({
                "status": "ok",
                "imported": imported,
                "skipped": skipped,
                "errors": errors,
            }))
        }
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

// =========================================================================
// Phase Omega — Decision Simulator
// =========================================================================

#[derive(Deserialize)]
struct SimulateReq { question: String }

async fn handle_simulate_decision(
    State(state): State<AppState>,
    Json(req): Json<SimulateReq>,
) -> impl IntoResponse {
    let _ = log_call(&state.db, "brain_simulate", &req.question).await;
    match crate::decision_simulator::simulate_decision(&state.db, &req.question).await {
        Ok(decision) => Json(serde_json::to_value(&decision).unwrap_or_default()).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// =========================================================================
// Phase Omega — Internal Dialogue
// =========================================================================

#[derive(Deserialize)]
struct DialogueReq { topic: String }

async fn handle_dialogue(
    State(state): State<AppState>,
    Json(req): Json<DialogueReq>,
) -> impl IntoResponse {
    let _ = log_call(&state.db, "brain_dialogue", &req.topic).await;
    match crate::internal_dialogue::run_dialogue(&state.db, &req.topic).await {
        Ok(dialogue) => Json(serde_json::to_value(&dialogue).unwrap_or_default()).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// =========================================================================
// Phase Omega — Cognitive Fingerprint
// =========================================================================

async fn handle_get_fingerprint(
    State(state): State<AppState>,
) -> impl IntoResponse {
    match crate::cognitive_fingerprint::get_fingerprint(&state.db).await {
        Ok(Some(fp)) => Json(serde_json::to_value(&fp).unwrap_or_default()).into_response(),
        Ok(None) => Json(serde_json::json!({ "status": "not_synthesized_yet" })).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn handle_synthesize_fingerprint(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let _ = log_call(&state.db, "brain_fingerprint_synthesize", "").await;
    match crate::cognitive_fingerprint::synthesize_fingerprint(&state.db).await {
        Ok(fp) => Json(serde_json::to_value(&fp).unwrap_or_default()).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// =========================================================================
// Phase Omega II — Swarm Orchestrator
// =========================================================================

async fn handle_swarm_status(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let _ = log_call(&state.db, "swarm_status", "").await;
    match crate::swarm::get_swarm_status_inner(&state.db).await {
        Ok(status) => Json(serde_json::to_value(&status).unwrap_or_default()).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
struct SwarmTaskReq {
    title: String,
    description: Option<String>,
    priority: Option<f32>,
    dependencies: Option<Vec<String>>,
}

async fn handle_swarm_create_task(
    State(state): State<AppState>,
    Json(req): Json<SwarmTaskReq>,
) -> impl IntoResponse {
    let _ = log_call(&state.db, "swarm_create_task", &req.title).await;
    match crate::swarm::create_task(
        &state.db,
        req.title,
        req.description.unwrap_or_default(),
        req.priority.unwrap_or(0.5),
        req.dependencies.unwrap_or_default(),
        None,
    ).await {
        Ok(task) => Json(serde_json::to_value(&task).unwrap_or_default()).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
struct SwarmGoalReq { goal: String }

async fn handle_swarm_decompose_goal(
    State(state): State<AppState>,
    Json(req): Json<SwarmGoalReq>,
) -> impl IntoResponse {
    let _ = log_call(&state.db, "swarm_decompose_goal", &req.goal).await;
    match crate::swarm::decompose_goal(&state.db, &req.goal).await {
        Ok(tasks) => Json(serde_json::to_value(&tasks).unwrap_or_default()).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// =========================================================================
// Phase Omega III — World Model
// =========================================================================

async fn handle_world_entities(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let _ = log_call(&state.db, "world_entities", "").await;
    match crate::world_model::get_all_entities(&state.db).await {
        Ok(entities) => Json(serde_json::to_value(&entities).unwrap_or_default()).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn handle_world_links(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let _ = log_call(&state.db, "world_links", "").await;
    match crate::world_model::get_all_links(&state.db).await {
        Ok(links) => Json(serde_json::to_value(&links).unwrap_or_default()).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
struct WorldSimulateReq { trigger: String }

async fn handle_world_simulate(
    State(state): State<AppState>,
    Json(req): Json<WorldSimulateReq>,
) -> impl IntoResponse {
    let _ = log_call(&state.db, "world_simulate", &req.trigger).await;
    match crate::world_model::simulate_scenario(&state.db, &req.trigger).await {
        Ok(prediction) => Json(serde_json::to_value(&prediction).unwrap_or_default()).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn handle_world_predictions(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let _ = log_call(&state.db, "world_predictions", "").await;
    match crate::temporal::get_predictions(&state.db).await {
        Ok(predictions) => Json(serde_json::to_value(&predictions).unwrap_or_default()).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// =========================================================================
// Phase Omega IV — Recursive Self-Improvement endpoints
// =========================================================================

async fn handle_self_rules(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let _ = log_call(&state.db, "self_rules", "").await;
    let rules: Result<Vec<crate::knowledge_compiler::KnowledgeRule>, _> = state.db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, source_node_ids, rule_type, condition, action,
                    confidence, times_applied, times_correct, accuracy,
                    compiled_at, invalidated
             FROM knowledge_rules WHERE invalidated = 0
             ORDER BY confidence DESC LIMIT 200"
        ).map_err(|e| crate::error::BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok(crate::knowledge_compiler::KnowledgeRule {
                id: row.get(0)?,
                source_node_ids: serde_json::from_str(&row.get::<_, String>(1)?).unwrap_or_default(),
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
        }).map_err(|e| crate::error::BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(v) = r { result.push(v); } }
        Ok(result)
    }).await;
    match rules {
        Ok(r) => Json(serde_json::json!({ "rules": r, "count": r.len() })).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn handle_self_performance(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let _ = log_call(&state.db, "self_performance", "").await;
    let perfs: Result<Vec<crate::circuit_performance::CircuitPerformance>, _> = state.db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT circuit_name, total_runs, success_runs, avg_duration_ms,
                    nodes_created, edges_created, iq_delta, efficiency
             FROM circuit_performance ORDER BY efficiency DESC"
        ).map_err(|e| crate::error::BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok(crate::circuit_performance::CircuitPerformance {
                circuit_name: row.get(0)?,
                total_runs: row.get(1)?,
                success_runs: row.get(2)?,
                avg_duration_ms: row.get::<_, i64>(3)? as u64,
                nodes_created: row.get(4)?,
                edges_created: row.get(5)?,
                iq_delta: row.get(6)?,
                efficiency: row.get(7)?,
            })
        }).map_err(|e| crate::error::BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(v) = r { result.push(v); } }
        Ok(result)
    }).await;
    match perfs {
        Ok(p) => Json(serde_json::json!({ "circuits": p, "count": p.len() })).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn handle_self_capabilities(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let _ = log_call(&state.db, "self_capabilities", "").await;
    let caps: Result<Vec<crate::capability_frontier::Capability>, _> = state.db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, name, proficiency, evidence_count, last_tested,
                    status, improvement_plan
             FROM capabilities ORDER BY proficiency DESC"
        ).map_err(|e| crate::error::BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok(crate::capability_frontier::Capability {
                id: row.get(0)?,
                name: row.get(1)?,
                proficiency: row.get(2)?,
                evidence_count: row.get(3)?,
                last_tested: row.get::<_, String>(4).unwrap_or_default(),
                status: row.get(5)?,
                improvement_plan: row.get(6)?,
            })
        }).map_err(|e| crate::error::BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(v) = r { result.push(v); } }
        Ok(result)
    }).await;
    match caps {
        Ok(c) => Json(serde_json::json!({ "capabilities": c, "count": c.len() })).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn handle_self_compile(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let _ = log_call(&state.db, "self_compile", "").await;
    let db_arc = state.db.clone();
    match crate::knowledge_compiler::compile_rules(&db_arc).await {
        Ok(result) => Json(serde_json::json!({ "status": "ok", "result": result })).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// =========================================================================
// Phase Omega IX — Consciousness Layer
// =========================================================================

async fn handle_consciousness_self(
    State(state): State<AppState>,
) -> impl IntoResponse {
    match crate::self_model::get_self_model(&state.db).await {
        Ok(Some(model)) => Json(serde_json::to_value(&model).unwrap_or_default()).into_response(),
        Ok(None) => {
            // Not built yet — build it now
            match crate::self_model::build_self_model(&state.db).await {
                Ok(model) => Json(serde_json::to_value(&model).unwrap_or_default()).into_response(),
                Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
            }
        }
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn handle_consciousness_attention(
    State(state): State<AppState>,
) -> impl IntoResponse {
    match crate::attention::get_focus_window(&state.db).await {
        Ok(window) => Json(serde_json::to_value(&window).unwrap_or_default()).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
struct CuriosityQuery {
    limit: Option<usize>,
}

async fn handle_consciousness_curiosity(
    State(state): State<AppState>,
    query: axum::extract::Query<CuriosityQuery>,
) -> impl IntoResponse {
    let limit = query.limit.unwrap_or(20);
    match crate::curiosity_v2::get_curiosity_targets(&state.db, limit).await {
        Ok(targets) => Json(serde_json::to_value(&targets).unwrap_or_default()).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// =========================================================================
// Phase Omega VII — Infrastructure handlers
// =========================================================================

async fn handle_infra_cluster(State(state): State<AppState>) -> impl IntoResponse {
    match crate::distributed::get_cluster_status(&state.db).await {
        Ok(status) => Json(serde_json::to_value(&status).unwrap_or_default()).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
struct RegisterNodeReq {
    name: String,
    role: String,
    endpoint_url: String,
}

async fn handle_infra_register_node(
    State(state): State<AppState>,
    Json(req): Json<RegisterNodeReq>,
) -> impl IntoResponse {
    crate::distributed::init_distributed(&state.db).await.ok();
    match crate::distributed::register_node(&state.db, req.name, req.role, req.endpoint_url).await
    {
        Ok(node) => Json(serde_json::to_value(&node).unwrap_or_default()).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn handle_infra_health(State(state): State<AppState>) -> impl IntoResponse {
    match crate::system_health::get_system_health(&state.db).await {
        Ok(health) => Json(serde_json::to_value(&health).unwrap_or_default()).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn handle_infra_edge_devices(State(state): State<AppState>) -> impl IntoResponse {
    match crate::edge_cache::get_edge_devices(&state.db).await {
        Ok(devices) => Json(serde_json::to_value(&devices).unwrap_or_default()).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// =========================================================================
// Phase Omega V — Sensory Expansion
// =========================================================================

#[derive(Deserialize)]
struct AnalyzeImageReq { image_path: String }

async fn handle_sensory_analyze_image(State(state): State<AppState>, Json(req): Json<AnalyzeImageReq>) -> impl IntoResponse {
    match crate::visual::analyze_image(&state.db, &req.image_path).await {
        Ok(analysis) => Json(serde_json::to_value(&analysis).unwrap_or_default()).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
struct TranscribeReq { audio_path: String }

async fn handle_sensory_transcribe(State(state): State<AppState>, Json(req): Json<TranscribeReq>) -> impl IntoResponse {
    match crate::audio::transcribe_audio(&state.db, &req.audio_path).await {
        Ok(transcription) => Json(serde_json::to_value(&transcription).unwrap_or_default()).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn handle_sensory_get_streams(State(state): State<AppState>) -> impl IntoResponse {
    match crate::data_streams::get_streams(&state.db).await {
        Ok(streams) => Json(serde_json::to_value(&streams).unwrap_or_default()).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
struct AddStreamReq { name: String, stream_type: String, url: String, interval: Option<u64> }

async fn handle_sensory_add_stream(State(state): State<AppState>, Json(req): Json<AddStreamReq>) -> impl IntoResponse {
    match crate::data_streams::add_stream(&state.db, req.name, req.stream_type, req.url, req.interval.unwrap_or(60)).await {
        Ok(stream) => Json(serde_json::to_value(&stream).unwrap_or_default()).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn handle_sensory_poll_streams(State(state): State<AppState>) -> impl IntoResponse {
    match crate::data_streams::poll_all_streams(&state.db).await {
        Ok(count) => Json(serde_json::json!({ "status": "ok", "new_items": count })).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// =========================================================================
// Phase Omega VI — Federation (The Collective)
// =========================================================================

async fn handle_federation_status(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let _ = log_call(&state.db, "federation_status", "").await;
    match crate::federation::get_federation_status(&state.db).await {
        Ok(status) => Json(serde_json::to_value(&status).unwrap_or_default()).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
struct FederationRegisterReq { name: String, endpoint_url: String }

async fn handle_federation_register(
    State(state): State<AppState>,
    Json(req): Json<FederationRegisterReq>,
) -> impl IntoResponse {
    let _ = log_call(&state.db, "federation_register", &req.name).await;
    match crate::federation::register_brain(&state.db, req.name, req.endpoint_url).await {
        Ok(brain) => Json(serde_json::to_value(&brain).unwrap_or_default()).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
struct FederationShareReq { brain_id: String, node_ids: Vec<String> }

async fn handle_federation_share(
    State(state): State<AppState>,
    Json(req): Json<FederationShareReq>,
) -> impl IntoResponse {
    let _ = log_call(&state.db, "federation_share", &req.brain_id).await;
    match crate::federation::share_knowledge(&state.db, &req.brain_id, req.node_ids).await {
        Ok(result) => Json(serde_json::json!({ "status": "ok", "result": result })).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
struct FederationSyncReq { from_brain: Option<String>, nodes: Option<Vec<serde_json::Value>>, since: Option<String> }

async fn handle_federation_sync(
    State(state): State<AppState>,
    Json(req): Json<FederationSyncReq>,
) -> impl IntoResponse {
    let _ = log_call(&state.db, "federation_sync", req.from_brain.as_deref().unwrap_or("unknown")).await;
    // If nodes are provided, this is an inbound sync — receive them and respond with our nodes
    if let Some(nodes) = req.nodes {
        let from = req.from_brain.unwrap_or_else(|| "unknown".to_string());
        let _ = crate::federation::receive_knowledge(&state.db, from, nodes).await;
    }
    // Return our shareable nodes since the given timestamp
    let since = req.since.unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string());
    let our_nodes: Vec<serde_json::Value> = state.db.with_conn(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, title, summary, domain, quality_score FROM nodes WHERE updated_at > ?1 AND quality_score > 0.4 ORDER BY quality_score DESC LIMIT 100"
        ).map_err(|e| crate::error::BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map(rusqlite::params![since], |row| {
            Ok(serde_json::json!({ "id": row.get::<_, String>(0)?, "title": row.get::<_, String>(1)?, "summary": row.get::<_, String>(2)?, "domain": row.get::<_, String>(3)?, "quality_score": row.get::<_, f64>(4)? }))
        }).map_err(|e| crate::error::BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(v) = r { result.push(v); } }
        Ok(result)
    }).await.unwrap_or_default();
    Json(serde_json::json!({ "nodes": our_nodes, "count": our_nodes.len() })).into_response()
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct FederationReceiveReq { message_id: Option<String>, from_brain: String, message_type: Option<String>, nodes: Vec<serde_json::Value> }

async fn handle_federation_receive(
    State(state): State<AppState>,
    Json(req): Json<FederationReceiveReq>,
) -> impl IntoResponse {
    let _ = log_call(&state.db, "federation_receive", &req.from_brain).await;
    match crate::federation::receive_knowledge(&state.db, req.from_brain, req.nodes).await {
        Ok(result) => Json(serde_json::json!({ "status": "ok", "result": result })).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// =========================================================================
// Phase Omega VIII — Economic Autonomy
// =========================================================================

#[derive(Deserialize)]
struct EconRevenueReq { source: String, amount: f64, currency: Option<String>, description: String, attributed_to: Option<String> }

async fn handle_economics_revenue(
    State(state): State<AppState>,
    Json(req): Json<EconRevenueReq>,
) -> impl IntoResponse {
    let _ = log_call(&state.db, "economics_revenue", &req.source).await;
    match crate::economics::record_revenue(&state.db, req.source, req.amount, req.currency.unwrap_or_else(|| "ZAR".to_string()), req.description, req.attributed_to).await {
        Ok(event) => Json(serde_json::to_value(&event).unwrap_or_default()).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
struct EconCostReq { cost_type: String, amount: f64, description: String }

async fn handle_economics_cost(
    State(state): State<AppState>,
    Json(req): Json<EconCostReq>,
) -> impl IntoResponse {
    let _ = log_call(&state.db, "economics_cost", &req.cost_type).await;
    match crate::economics::record_cost(&state.db, req.cost_type, req.amount, req.description).await {
        Ok(cost) => Json(serde_json::to_value(&cost).unwrap_or_default()).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
struct EconReportQuery { period_days: Option<u32> }

async fn handle_economics_report(
    State(state): State<AppState>,
    query: axum::extract::Query<EconReportQuery>,
) -> impl IntoResponse {
    let _ = log_call(&state.db, "economics_report", "").await;
    let period = query.period_days.unwrap_or(30);
    match crate::economics::generate_economic_report(&state.db, period).await {
        Ok(report) => Json(serde_json::to_value(&report).unwrap_or_default()).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn handle_economics_sustaining(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let _ = log_call(&state.db, "economics_sustaining", "").await;
    match crate::economics::is_self_sustaining(&state.db).await {
        Ok(sustaining) => Json(serde_json::json!({ "self_sustaining": sustaining })).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// =========================================================================
// Dual-Brain — Context Bundle endpoint
// =========================================================================

#[derive(Deserialize)]
struct BundleReq { query: String, project: Option<String> }

async fn handle_context_bundle(
    State(state): State<AppState>,
    Json(req): Json<BundleReq>,
) -> impl IntoResponse {
    let _ = log_call(&state.db, "brain_bundle", &req.query).await;
    let project = req.project.unwrap_or_else(|| "unknown".to_string());
    let bundle = crate::context_bundle::build_context_bundle(&state.db, &req.query, &project).await;
    let md = crate::context_bundle::render_sidekick_context(&bundle);
    Json(serde_json::json!({
        "markdown": md,
        "stats": {
            "rules": bundle.compiled_rules.len(),
            "knowledge_nodes": bundle.knowledge_nodes.len(),
            "work_patterns": bundle.work_patterns.len(),
            "decisions": bundle.decisions.len(),
            "warnings": bundle.warnings.len(),
            "predictions": bundle.predictions.len(),
            "total_chars": bundle.total_chars,
            "generation_ms": bundle.generation_ms,
        }
    })).into_response()
}

// =========================================================================
// Phase 2 — Brain Learning Tools
// =========================================================================

#[derive(Deserialize)]
struct WarningsReq { query: String }

async fn handle_brain_warnings(
    State(state): State<AppState>,
    Json(req): Json<WarningsReq>,
) -> impl IntoResponse {
    let _ = log_call(&state.db, "brain_warnings", &req.query).await;
    let q = req.query;
    let results: Vec<serde_json::Value> = state.db.with_conn(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT title, summary, content, tags, quality_score FROM nodes
             WHERE (node_type = 'contradiction' OR cognitive_type = 'contradiction'
                    OR tags LIKE '%warning%' OR tags LIKE '%mistake%'
                    OR tags LIKE '%gotcha%' OR tags LIKE '%bug%')
             AND (title LIKE '%' || ?1 || '%' OR content LIKE '%' || ?1 || '%')
             ORDER BY quality_score DESC LIMIT 10"
        ).map_err(|e| crate::error::BrainError::Database(e.to_string()))?;
        let search = q.split_whitespace().take(3).collect::<Vec<_>>().join(" ");
        let rows = stmt.query_map(params![search], |row| {
            Ok(serde_json::json!({
                "title": row.get::<_, String>(0)?,
                "summary": row.get::<_, String>(1)?,
                "content": crate::truncate_str(&row.get::<_, String>(2)?, 500),
                "tags": row.get::<_, String>(3)?,
                "quality": row.get::<_, f32>(4)?,
            }))
        }).map_err(|e| crate::error::BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(v) = r { result.push(v); } }
        Ok(result)
    }).await.unwrap_or_default();
    Json(serde_json::json!({ "warnings": results, "count": results.len() })).into_response()
}

#[derive(Deserialize)]
struct RulesReq { context: Option<String> }

async fn handle_brain_rules(
    State(state): State<AppState>,
    Json(req): Json<RulesReq>,
) -> impl IntoResponse {
    let _ = log_call(&state.db, "brain_rules", req.context.as_deref().unwrap_or("")).await;
    let rules: Vec<serde_json::Value> = state.db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT rule_type, condition, action, confidence, times_applied, accuracy
             FROM knowledge_rules WHERE invalidated = 0
             ORDER BY confidence DESC LIMIT 50"
        ).map_err(|e| crate::error::BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok(serde_json::json!({
                "rule_type": row.get::<_, String>(0)?,
                "condition": row.get::<_, String>(1)?,
                "action": row.get::<_, String>(2)?,
                "confidence": row.get::<_, f32>(3)?,
                "times_applied": row.get::<_, u32>(4)?,
                "accuracy": row.get::<_, f32>(5)?,
            }))
        }).map_err(|e| crate::error::BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(v) = r { result.push(v); } }
        Ok(result)
    }).await.unwrap_or_default();

    // If context provided, filter to matching rules
    let filtered = if let Some(ctx) = &req.context {
        let ctx_lower = ctx.to_lowercase();
        let ctx_words: std::collections::HashSet<String> = ctx_lower
            .split_whitespace().filter(|w| w.len() > 2)
            .map(|s| s.trim_matches(|c: char| !c.is_alphanumeric()).to_string())
            .filter(|s| !s.is_empty()).collect();
        rules.into_iter().filter(|r| {
            let cond = r["condition"].as_str().unwrap_or("").to_lowercase();
            let cond_words: std::collections::HashSet<String> = cond
                .split(|c: char| !c.is_alphanumeric() && c != '_')
                .filter(|w| w.len() > 2).map(|s| s.to_string()).collect();
            ctx_words.intersection(&cond_words).count() >= 2
        }).collect()
    } else {
        rules
    };

    Json(serde_json::json!({ "rules": filtered, "count": filtered.len() })).into_response()
}

#[derive(Deserialize)]
struct LearnDecisionReq {
    topic: String,
    choice: String,
    reasoning: String,
    alternatives: Option<Vec<String>>,
    confidence: Option<f32>,
}

async fn handle_learn_decision(
    State(state): State<AppState>,
    Json(req): Json<LearnDecisionReq>,
) -> impl IntoResponse {
    let _ = log_call(&state.db, "brain_learn_decision", &req.topic).await;
    let now = chrono::Utc::now().to_rfc3339();
    let node_id = format!("node:{}", uuid::Uuid::now_v7());
    let alts = req.alternatives.unwrap_or_default().join(", ");
    let content = format!(
        "Decision: {}\n\nChose: {}\nAlternatives considered: {}\n\nReasoning: {}",
        req.topic, req.choice, alts, req.reasoning
    );
    let conf = req.confidence.unwrap_or(0.8);
    let tags = serde_json::to_string(&vec!["decision", "manual", "claude-learned"]).unwrap_or_default();
    let nid = node_id.clone();
    let content_hash = format!("{:x}", sha2::Sha256::digest(content.as_bytes()));
    let res = state.db.with_conn(move |conn| {
        conn.execute(
            "INSERT INTO nodes (id, title, content, summary, content_hash, domain, topic, tags,
                                node_type, source_type, quality_score, visual_size,
                                decay_score, access_count, synthesized_by_brain, cognitive_type,
                                confidence, created_at, updated_at, accessed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'technology', ?6, ?7, 'decision', 'mcp_learn', ?8, 3.0, 1.0, 0, 0, 'decision', ?8, ?9, ?9, ?9)",
            params![nid, format!("Decision: {}", req.topic), content,
                    format!("Chose {} over {}", req.choice, alts),
                    content_hash,
                    req.topic.to_lowercase().replace(' ', "-"), tags, conf, now],
        ).map_err(|e| crate::error::BrainError::Database(e.to_string()))?;
        Ok(())
    }).await;
    match res {
        Ok(()) => Json(serde_json::json!({ "stored": true, "node_id": node_id })).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
struct LearnPatternReq {
    observation: String,
    pattern_type: Option<String>,
    confidence: Option<f32>,
}

async fn handle_learn_pattern(
    State(state): State<AppState>,
    Json(req): Json<LearnPatternReq>,
) -> impl IntoResponse {
    let _ = log_call(&state.db, "brain_learn_pattern", &req.observation).await;
    let now = chrono::Utc::now().to_rfc3339();
    let id = format!("uc:{}", uuid::Uuid::now_v7());
    let ptype = req.pattern_type.unwrap_or_else(|| "general".to_string());
    let conf = req.confidence.unwrap_or(0.7);
    let rid = id.clone();
    let pt_clone = ptype.clone();
    let res = state.db.with_conn(move |conn| {
        conn.execute(
            "INSERT INTO user_cognition (id, timestamp, trigger_node_ids, pattern_type,
                                         extracted_rule, structured_rule, confidence,
                                         times_confirmed, times_contradicted, linked_to_nodes)
             VALUES (?1, ?2, '[]', ?3, ?4, NULL, ?5, 1, 0, '[]')",
            params![rid, now, pt_clone, req.observation, conf],
        ).map_err(|e| crate::error::BrainError::Database(e.to_string()))?;
        Ok(())
    }).await;
    match res {
        Ok(()) => Json(serde_json::json!({ "stored": true, "cognition_id": id, "pattern_type": ptype })).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
struct LearnMistakeReq {
    title: String,
    description: String,
    severity: Option<String>,
}

async fn handle_learn_mistake(
    State(state): State<AppState>,
    Json(req): Json<LearnMistakeReq>,
) -> impl IntoResponse {
    let _ = log_call(&state.db, "brain_learn_mistake", &req.title).await;
    let now = chrono::Utc::now().to_rfc3339();
    let node_id = format!("node:{}", uuid::Uuid::now_v7());
    let sev = req.severity.unwrap_or_else(|| "medium".to_string());
    let tags = serde_json::to_string(&vec!["warning", "mistake", "claude-learned", &sev]).unwrap_or_default();
    let nid = node_id.clone();
    let sev_clone = sev.clone();
    let title_clone = req.title.clone();
    let content_hash = format!("{:x}", sha2::Sha256::digest(req.description.as_bytes()));
    let res = state.db.with_conn(move |conn| {
        conn.execute(
            "INSERT INTO nodes (id, title, content, summary, content_hash, domain, topic, tags,
                                node_type, source_type, quality_score, visual_size,
                                decay_score, access_count, synthesized_by_brain, cognitive_type,
                                created_at, updated_at, accessed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'technology', 'warnings', ?6, 'contradiction', 'mcp_learn', 0.9, 4.0, 1.0, 0, 0, 'contradiction', ?7, ?7, ?7)",
            params![nid, format!("Warning: {}", title_clone), req.description,
                    format!("[{}] {}", sev_clone.to_uppercase(), title_clone), content_hash, tags, now],
        ).map_err(|e| crate::error::BrainError::Database(e.to_string()))?;
        Ok(())
    }).await;
    match res {
        Ok(()) => Json(serde_json::json!({ "stored": true, "node_id": node_id, "severity": sev })).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// =========================================================================
// Phase 3 — Session Handoff
// =========================================================================

async fn handle_session_handoff(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let path = state.db.config.export_dir().join("session-handoff.md");
    match std::fs::read_to_string(&path) {
        Ok(content) => Json(serde_json::json!({ "handoff": content })).into_response(),
        Err(_) => Json(serde_json::json!({ "handoff": null, "message": "No session handoff available yet. The session_summarizer circuit creates this after a completed session." })).into_response(),
    }
}
