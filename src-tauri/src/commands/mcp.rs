//! MCP (Model Context Protocol) commands — the bridge between your AI assistant
//! and the brain.

use crate::db::models::{SearchResult, UserCognition};
use crate::db::BrainDb;
use crate::error::BrainError;
use crate::mcp::McpConnection;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

// =========================================================================
// Status (existing)
// =========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpStatus {
    pub connections: Vec<McpConnection>,
}

#[tauri::command]
pub async fn get_mcp_status() -> Result<McpStatus, BrainError> {
    Ok(McpStatus {
        connections: vec![
            McpConnection {
                name: "Context7".to_string(),
                status: "available".to_string(),
                server_type: "context7".to_string(),
                last_used: None,
            },
            McpConnection {
                name: "GitHub".to_string(),
                status: "available".to_string(),
                server_type: "github".to_string(),
                last_used: None,
            },
            McpConnection {
                name: "Brain MCP Server".to_string(),
                status: "stub_active".to_string(),
                server_type: "brain".to_string(),
                last_used: None,
            },
        ],
    })
}

// =========================================================================
// Brain MCP commands
// =========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainRecallResult {
    pub query: String,
    pub matches: Vec<SearchResult>,
    pub synthesis: Option<String>,
    pub source_count: usize,
}

#[tauri::command]
pub async fn brain_recall(
    db: State<'_, Arc<BrainDb>>,
    query: String,
    limit: Option<usize>,
) -> Result<BrainRecallResult, BrainError> {
    log_mcp_call(&db, "brain_recall", &query).await;
    let limit = limit.unwrap_or(10);

    let client = crate::embeddings::OllamaClient::new(
        db.config.ollama_url.clone(),
        db.config.embedding_model.clone(),
    );
    let matches = if client.health_check().await {
        match client.generate_embedding(&query).await {
            Ok(embedding) => db.vector_search(embedding, limit).await.unwrap_or_default(),
            Err(_) => db.search_nodes(&query).await.unwrap_or_default(),
        }
    } else {
        db.search_nodes(&query).await.unwrap_or_default()
    };

    let source_count = matches.len();
    Ok(BrainRecallResult {
        query,
        matches,
        synthesis: None,
        source_count,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainContextResult {
    pub file_path: String,
    pub matches: Vec<SearchResult>,
    pub source_count: usize,
}

#[tauri::command]
pub async fn brain_context(
    db: State<'_, Arc<BrainDb>>,
    file_path: String,
) -> Result<BrainContextResult, BrainError> {
    log_mcp_call(&db, "brain_context", &file_path).await;

    let path = std::path::Path::new(&file_path);
    let filename = path.file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(&file_path)
        .to_string();
    let parent = path.parent()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let query = if parent.is_empty() {
        filename.clone()
    } else {
        format!("{} {}", parent, filename)
    };

    let matches = db.search_nodes(&query).await.unwrap_or_default();
    let source_count = matches.len();
    Ok(BrainContextResult { file_path, matches, source_count })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainPreferencesResult {
    pub rules: Vec<UserCognition>,
    pub total_count: usize,
    pub by_pattern_type: std::collections::HashMap<String, usize>,
}

#[tauri::command]
pub async fn brain_preferences(
    db: State<'_, Arc<BrainDb>>,
    pattern_type: Option<String>,
) -> Result<BrainPreferencesResult, BrainError> {
    log_mcp_call(&db, "brain_preferences", pattern_type.as_deref().unwrap_or("ALL")).await;

    let pt = pattern_type.clone();
    let rules: Vec<UserCognition> = db.with_conn(move |conn| {
        let mut result = Vec::new();
        if let Some(pt) = &pt {
            let mut stmt = conn.prepare(
                "SELECT id, timestamp, trigger_node_ids, pattern_type, extracted_rule, \
                 structured_rule, confidence, times_confirmed, times_contradicted, \
                 embedding, linked_to_nodes FROM user_cognition \
                 WHERE pattern_type = ?1 LIMIT 100"
            ).map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt.query_map(params![pt], |row| {
                Ok(crate::db::models::UserCognition {
                    id: row.get(0)?,
                    timestamp: row.get(1)?,
                    trigger_node_ids: serde_json::from_str(&row.get::<_, String>(2)?).unwrap_or_default(),
                    pattern_type: row.get(3)?,
                    extracted_rule: row.get(4)?,
                    structured_rule: row.get(5)?,
                    confidence: row.get(6)?,
                    times_confirmed: row.get(7)?,
                    times_contradicted: row.get(8)?,
                    embedding: None,
                    linked_to_nodes: serde_json::from_str(&row.get::<_, String>(10)?).unwrap_or_default(),
                })
            }).map_err(|e| BrainError::Database(e.to_string()))?;
            for r in rows { if let Ok(c) = r { result.push(c); } }
        } else {
            let mut stmt = conn.prepare(
                "SELECT id, timestamp, trigger_node_ids, pattern_type, extracted_rule, \
                 structured_rule, confidence, times_confirmed, times_contradicted, \
                 embedding, linked_to_nodes FROM user_cognition LIMIT 200"
            ).map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt.query_map([], |row| {
                Ok(crate::db::models::UserCognition {
                    id: row.get(0)?,
                    timestamp: row.get(1)?,
                    trigger_node_ids: serde_json::from_str(&row.get::<_, String>(2)?).unwrap_or_default(),
                    pattern_type: row.get(3)?,
                    extracted_rule: row.get(4)?,
                    structured_rule: row.get(5)?,
                    confidence: row.get(6)?,
                    times_confirmed: row.get(7)?,
                    times_contradicted: row.get(8)?,
                    embedding: None,
                    linked_to_nodes: serde_json::from_str(&row.get::<_, String>(10)?).unwrap_or_default(),
                })
            }).map_err(|e| BrainError::Database(e.to_string()))?;
            for r in rows { if let Ok(c) = r { result.push(c); } }
        }
        Ok(result)
    }).await?;

    let mut by_type: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for r in &rules {
        *by_type.entry(r.pattern_type.clone()).or_insert(0) += 1;
    }

    let mut sorted = rules;
    sorted.sort_by(|a, b| {
        let score_a = a.confidence * (a.times_confirmed as f32 + 1.0);
        let score_b = b.confidence * (b.times_confirmed as f32 + 1.0);
        score_b.partial_cmp(&score_a).unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(BrainPreferencesResult {
        total_count: sorted.len(),
        by_pattern_type: by_type,
        rules: sorted,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainDecisionsResult {
    pub topic: String,
    pub decisions: Vec<SearchResult>,
    pub source_count: usize,
}

#[tauri::command]
pub async fn brain_decisions(
    db: State<'_, Arc<BrainDb>>,
    topic: String,
) -> Result<BrainDecisionsResult, BrainError> {
    log_mcp_call(&db, "brain_decisions", &topic).await;

    let all_matches = db.search_nodes(&topic).await.unwrap_or_default();
    let decisions: Vec<SearchResult> = all_matches
        .into_iter()
        .filter(|m| {
            m.node.node_type == crate::db::models::NODE_TYPE_DECISION
                || m.node.tags.iter().any(|t| t == "decision")
        })
        .take(10)
        .collect();

    let source_count = decisions.len();
    Ok(BrainDecisionsResult { topic, decisions, source_count })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainLearnInput {
    pub observation: String,
    pub pattern_type: Option<String>,
    pub trigger_node_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainLearnResult {
    pub stored_id: String,
    pub action: String,
}

#[tauri::command]
pub async fn brain_learn(
    db: State<'_, Arc<BrainDb>>,
    input: BrainLearnInput,
) -> Result<BrainLearnResult, BrainError> {
    log_mcp_call(&db, "brain_learn", &input.observation).await;

    let pattern_type = input
        .pattern_type
        .unwrap_or_else(|| "general".to_string())
        .to_lowercase();
    let now = chrono::Utc::now().to_rfc3339();
    let trigger_ids = input
        .trigger_node_id
        .map(|id| vec![id])
        .unwrap_or_default();
    let trigger_json = serde_json::to_string(&trigger_ids).unwrap_or_else(|_| "[]".to_string());

    let id = format!("user_cognition:{}", uuid::Uuid::now_v7());
    let stored_id = id.clone();
    let rule = input.observation.clone();

    db.with_conn(move |conn| {
        conn.execute(
            "INSERT INTO user_cognition (id, timestamp, trigger_node_ids, pattern_type, \
             extracted_rule, structured_rule, confidence, times_confirmed, times_contradicted, \
             embedding, linked_to_nodes) VALUES (?1, ?2, ?3, ?4, ?5, NULL, 0.7, 1, 0, NULL, '[]')",
            params![id, now, trigger_json, pattern_type, rule],
        ).map_err(|e| BrainError::Database(e.to_string()))
    }).await?;

    Ok(BrainLearnResult {
        stored_id,
        action: "created".to_string(),
    })
}

// =========================================================================
// PHASE 4.9 — Additional MCP brain tools
// =========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainCritiqueResult {
    pub text: String,
    pub matches_user_patterns: Vec<UserCognition>,
    pub conflicts_with_user_patterns: Vec<UserCognition>,
    pub summary: String,
}

#[tauri::command]
pub async fn brain_critique(
    db: State<'_, Arc<BrainDb>>,
    text: String,
) -> Result<BrainCritiqueResult, BrainError> {
    log_mcp_call(&db, "brain_critique", &text).await;

    let rules: Vec<UserCognition> = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, timestamp, trigger_node_ids, pattern_type, extracted_rule, \
             structured_rule, confidence, times_confirmed, times_contradicted, \
             embedding, linked_to_nodes FROM user_cognition WHERE confidence > 0.5 LIMIT 200"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok(UserCognition {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                trigger_node_ids: serde_json::from_str(&row.get::<_, String>(2)?).unwrap_or_default(),
                pattern_type: row.get(3)?,
                extracted_rule: row.get(4)?,
                structured_rule: row.get(5)?,
                confidence: row.get(6)?,
                times_confirmed: row.get(7)?,
                times_contradicted: row.get(8)?,
                embedding: None,
                linked_to_nodes: serde_json::from_str(&row.get::<_, String>(10)?).unwrap_or_default(),
            })
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(c) = r { result.push(c); } }
        Ok(result)
    }).await?;

    let text_lower = text.to_lowercase();
    let text_words: std::collections::HashSet<String> = text_lower
        .split_whitespace()
        .filter(|w| w.len() > 4)
        .map(|s| s.trim_matches(|c: char| !c.is_alphanumeric()).to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let mut matches = Vec::new();
    let mut conflicts = Vec::new();

    for rule in rules {
        let rule_lower = rule.extracted_rule.to_lowercase();
        let rule_words: std::collections::HashSet<String> = rule_lower
            .split_whitespace()
            .filter(|w| w.len() > 4)
            .map(|s| s.trim_matches(|c: char| !c.is_alphanumeric()).to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let overlap = text_words.intersection(&rule_words).count();
        if overlap >= 2 {
            let rule_negative = rule_lower.contains("don't")
                || rule_lower.contains("never")
                || rule_lower.contains("avoid")
                || rule_lower.contains("not ");
            if rule_negative {
                conflicts.push(rule);
            } else {
                matches.push(rule);
            }
        }
    }

    matches.truncate(8);
    conflicts.truncate(8);
    let summary = format!(
        "Found {} aligned patterns and {} potential conflicts",
        matches.len(), conflicts.len()
    );

    Ok(BrainCritiqueResult {
        text,
        matches_user_patterns: matches,
        conflicts_with_user_patterns: conflicts,
        summary,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainHistoryEntry {
    pub timestamp: String,
    pub title: String,
    pub node_type: String,
    pub source_type: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainHistoryResult {
    pub topic: String,
    pub timeline: Vec<BrainHistoryEntry>,
    pub source_count: usize,
}

#[tauri::command]
pub async fn brain_history(
    db: State<'_, Arc<BrainDb>>,
    topic: String,
) -> Result<BrainHistoryResult, BrainError> {
    log_mcp_call(&db, "brain_history", &topic).await;

    let topic_clone = topic.clone();
    let mut rows: Vec<BrainHistoryEntry> = db.with_conn(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT title, node_type, source_type, summary, created_at FROM nodes \
             WHERE topic = ?1 ORDER BY created_at ASC LIMIT 50"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let mapped = stmt.query_map(params![topic_clone], |row| {
            Ok(BrainHistoryEntry {
                title: row.get(0)?,
                node_type: row.get(1)?,
                source_type: row.get(2)?,
                summary: row.get(3)?,
                timestamp: row.get(4)?,
            })
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in mapped { if let Ok(e) = r { result.push(e); } }
        Ok(result)
    }).await?;

    if rows.is_empty() {
        let search_results = db.search_nodes(&topic).await.unwrap_or_default();
        for sr in search_results.into_iter().take(50) {
            rows.push(BrainHistoryEntry {
                title: sr.node.title,
                node_type: sr.node.node_type,
                source_type: sr.node.source_type,
                summary: sr.node.summary,
                timestamp: sr.node.created_at,
            });
        }
        rows.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
    }

    let source_count = rows.len();
    Ok(BrainHistoryResult { topic, timeline: rows, source_count })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainSubgraphResult {
    pub node_ids: Vec<String>,
    pub nodes: Vec<crate::db::models::GraphNode>,
    pub edges: Vec<crate::db::models::GraphEdge>,
    pub source_count: usize,
}

#[tauri::command]
pub async fn brain_export_subgraph(
    db: State<'_, Arc<BrainDb>>,
    node_ids: Vec<String>,
) -> Result<BrainSubgraphResult, BrainError> {
    log_mcp_call(&db, "brain_export_subgraph", &node_ids.join(",")).await;

    let mut all_node_ids: std::collections::HashSet<String> = node_ids.iter().cloned().collect();
    let mut edges_out: Vec<crate::db::models::GraphEdge> = Vec::new();

    for nid in &node_ids {
        let neighbours = db.get_edges_for_node(nid).await.unwrap_or_default();
        for e in neighbours {
            all_node_ids.insert(e.source.clone());
            all_node_ids.insert(e.target.clone());
            edges_out.push(e);
        }
    }

    let nodes_out: Vec<crate::db::models::GraphNode>;
    let ids_vec: Vec<String> = all_node_ids.into_iter().collect();
    let fetched_nodes = db.with_conn(move |conn| {
        let mut result = Vec::new();
        for id in &ids_vec {
            let mut stmt = conn.prepare(
                "SELECT id, title, content, summary, domain, topic, tags, node_type, \
                 source_type, visual_size, access_count, decay_score, created_at \
                 FROM nodes WHERE id = ?1"
            ).map_err(|e| BrainError::Database(e.to_string()))?;
            let mut rows = stmt.query_map(params![id], |row| {
                let tags_json: String = row.get(6)?;
                Ok(crate::db::models::GraphNode {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    content: row.get(2)?,
                    summary: row.get(3)?,
                    domain: row.get(4)?,
                    topic: row.get(5)?,
                    tags: serde_json::from_str(&tags_json).unwrap_or_default(),
                    node_type: row.get(7)?,
                    source_type: row.get(8)?,
                    visual_size: row.get(9)?,
                    access_count: row.get(10)?,
                    decay_score: row.get(11)?,
                    created_at: row.get(12)?,
                })
            }).map_err(|e| BrainError::Database(e.to_string()))?;
            if let Some(Ok(n)) = rows.next() {
                result.push(n);
            }
        }
        Ok(result)
    }).await?;
    nodes_out = fetched_nodes;

    let source_count = nodes_out.len();
    Ok(BrainSubgraphResult {
        node_ids,
        nodes: nodes_out,
        edges: edges_out,
        source_count,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainPlanResult {
    pub task: String,
    pub plan: String,
    pub used_nodes: Vec<String>,
    pub used_preferences: Vec<String>,
}

#[tauri::command]
pub async fn brain_plan(
    db: State<'_, Arc<BrainDb>>,
    task: String,
) -> Result<BrainPlanResult, BrainError> {
    log_mcp_call(&db, "brain_plan", &task).await;

    let client = crate::embeddings::OllamaClient::new(
        db.config.ollama_url.clone(),
        db.config.embedding_model.clone(),
    );
    let related = if client.health_check().await {
        match client.generate_embedding(&task).await {
            Ok(emb) => db.vector_search(emb, 5).await.unwrap_or_default(),
            Err(_) => db.search_nodes(&task).await.unwrap_or_default(),
        }
    } else {
        db.search_nodes(&task).await.unwrap_or_default()
    };

    let rules: Vec<UserCognition> = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, timestamp, trigger_node_ids, pattern_type, extracted_rule, \
             structured_rule, confidence, times_confirmed, times_contradicted, \
             embedding, linked_to_nodes FROM user_cognition \
             WHERE confidence > 0.6 ORDER BY confidence DESC LIMIT 10"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok(UserCognition {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                trigger_node_ids: serde_json::from_str(&row.get::<_, String>(2)?).unwrap_or_default(),
                pattern_type: row.get(3)?,
                extracted_rule: row.get(4)?,
                structured_rule: row.get(5)?,
                confidence: row.get(6)?,
                times_confirmed: row.get(7)?,
                times_contradicted: row.get(8)?,
                embedding: None,
                linked_to_nodes: serde_json::from_str(&row.get::<_, String>(10)?).unwrap_or_default(),
            })
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(c) = r { result.push(c); } }
        Ok(result)
    }).await.unwrap_or_default();

    let mut context = String::new();
    if !related.is_empty() {
        context.push_str("RELEVANT PAST KNOWLEDGE:\n");
        for r in related.iter().take(5) {
            context.push_str(&format!("- {}: {}\n", r.node.title, r.node.summary));
        }
        context.push('\n');
    }
    if !rules.is_empty() {
        context.push_str("HEIN'S ESTABLISHED PREFERENCES:\n");
        for r in rules.iter().take(5) {
            context.push_str(&format!("- ({}) {}\n", r.pattern_type, r.extracted_rule));
        }
        context.push('\n');
    }

    let llm = crate::commands::ai::get_llm_client_fast(&db);
    let prompt = format!(
        "You are NeuroVault's planner. Generate a concrete step-by-step plan for the task below, \
         following the user's established preferences and reusing relevant past decisions. \
         Output as a numbered list, 4-8 steps. No preamble.\n\n\
         {}\n\n\
         TASK: {}",
        context, task
    );

    let plan = llm.generate(&prompt, 600).await?;

    let used_nodes: Vec<String> = related.iter().take(5).map(|r| r.node.title.clone()).collect();
    let used_preferences: Vec<String> = rules.iter().take(5).map(|r| r.extracted_rule.clone()).collect();

    Ok(BrainPlanResult {
        task,
        plan,
        used_nodes,
        used_preferences,
    })
}

// =========================================================================
// Logging
// =========================================================================

async fn log_mcp_call(db: &BrainDb, command: &str, payload: &str) {
    let now = chrono::Utc::now().to_rfc3339();
    let payload_trunc = if payload.len() > 500 {
        format!("{}...", &payload[..500])
    } else {
        payload.to_string()
    };
    let id = format!("mcp_call_log:{}", uuid::Uuid::now_v7());
    let cmd = command.to_string();
    let _ = db.with_conn(move |conn| {
        conn.execute(
            "INSERT INTO mcp_call_log (id, tool_name, args, result, called_at) VALUES (?1, ?2, ?3, '', ?4)",
            params![id, cmd, payload_trunc, now],
        ).map_err(|e| BrainError::Database(e.to_string()))
    }).await;
}
