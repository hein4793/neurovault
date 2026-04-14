//! Sidekick Suggestion Engine — Phase 1.2C of the master plan.
//!
//! Companion to the proactive context injector (`sidekick.rs`). While
//! the context injector writes a snapshot of "what does the brain know
//! about the current task", this module generates **categorized
//! suggestions** in three flavors:
//!
//!   - **Insights** — "You solved something similar in project X..."
//!   - **Warnings** — "This approach conflicts with your decision in Y..."
//!   - **Optimizations** — "Based on your patterns, consider Z instead..."
//!
//! Output goes to `~/.neurovault/export/active-suggestions.md` and is
//! also queryable via the new `get_active_suggestions` Tauri command.
//!
//! ## How it works
//!
//! Runs every ~60 seconds (less aggressive than the context injector
//! because LLM calls are involved). Each cycle:
//!
//! 1. Reads the most recently active Claude Code session's last few messages
//! 2. Pulls semantically related nodes via vector_search
//! 3. Pulls top user_cognition rules
//! 4. Asks the LLM (FAST tier) to categorize relevant items into the
//!    three buckets
//! 5. Writes a markdown file with the categorized output
//!
//! ## Why it's separate from the context injector
//!
//! - **Cost**: this module makes one LLM call per cycle. The context
//!   injector is LLM-free (just semantic search + file write).
//! - **Cadence**: context refreshes every 30s; suggestions refresh
//!   every 60s.
//! - **Output shape**: context is a knowledge dump; suggestions are
//!   actionable bullets with verbs.

use crate::commands::ai::get_llm_client_fast;
use crate::db::models::{SearchResult, UserCognition};
use crate::db::BrainDb;
use crate::error::BrainError;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ActiveSuggestions {
    pub generated_at: String,
    pub project: String,
    pub session: String,
    pub query_basis: String,
    pub insights: Vec<String>,
    pub warnings: Vec<String>,
    pub optimizations: Vec<String>,
}

/// Spawn the suggestion engine forever. Called from lib.rs setup().
pub async fn run_suggestion_engine(db: Arc<BrainDb>) {
    // Wait 90s after startup so the rest of the system is steady-state
    // (HNSW build, embedding pipeline first cycle, sidekick startup)
    tokio::time::sleep(Duration::from_secs(90)).await;
    log::info!("Sidekick suggestion engine started — categorized suggestions every 60s");

    let mut last_session: Option<PathBuf> = None;
    let mut last_message_count: usize = 0;

    loop {
        if let Err(e) = run_one_cycle(&db, &mut last_session, &mut last_message_count).await {
            log::warn!("Sidekick suggestion cycle failed: {}", e);
        }
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}

async fn run_one_cycle(
    db: &BrainDb,
    last_session: &mut Option<PathBuf>,
    last_message_count: &mut usize,
) -> Result<(), String> {
    // Reuse the sidekick module's session detection
    let session = crate::sidekick::find_active_session_pub()
        .ok_or_else(|| "no active Claude Code session found".to_string())?;

    let session_changed = last_session.as_deref() != Some(session.as_path());
    *last_session = Some(session.clone());

    let messages: Vec<(String, String)> = crate::sidekick::read_recent_messages_pub(&session, 8)
        .map_err(|e| format!("read messages: {}", e))?;
    if messages.is_empty() {
        return Ok(());
    }

    if !session_changed && messages.len() == *last_message_count {
        return Ok(());
    }
    *last_message_count = messages.len();

    let query = crate::sidekick::extract_context_query_pub(&messages);
    if query.trim().len() < 5 {
        return Ok(());
    }

    // Pull related context
    let client = crate::embeddings::OllamaClient::new(
        db.config.ollama_url.clone(),
        db.config.embedding_model.clone(),
    );
    let related: Vec<SearchResult> = if client.health_check().await {
        match client.generate_embedding(&query).await {
            Ok(emb) => db.vector_search(emb, 10).await.unwrap_or_default(),
            Err(_) => db.search_nodes(&query).await.unwrap_or_default(),
        }
    } else {
        db.search_nodes(&query).await.unwrap_or_default()
    };

    // Pull top user_cognition rules
    let rules: Vec<UserCognition> = db.with_conn(|conn| -> Result<Vec<UserCognition>, BrainError> {
        let mut stmt = conn.prepare(
            "SELECT id, timestamp, trigger_node_ids, pattern_type, extracted_rule, \
             structured_rule, confidence, times_confirmed, times_contradicted, \
             embedding, linked_to_nodes \
             FROM user_cognition WHERE confidence > 0.6 \
             ORDER BY confidence DESC LIMIT 15"
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
        for r in rows { if let Ok(n) = r { result.push(n); } }
        Ok(result)
    }).await.unwrap_or_default();

    // Build context for the LLM
    let mut context = String::new();
    if !related.is_empty() {
        context.push_str("RELEVANT PAST KNOWLEDGE:\n");
        for r in related.iter().take(8) {
            context.push_str(&format!(
                "- [{}] {}: {}\n",
                r.node.domain, r.node.title, short(&r.node.summary, 180)
            ));
        }
        context.push('\n');
    }
    if !rules.is_empty() {
        context.push_str("ESTABLISHED USER PATTERNS:\n");
        for r in rules.iter().take(8) {
            context.push_str(&format!("- ({}) {}\n", r.pattern_type, r.extracted_rule));
        }
        context.push('\n');
    }

    // Ask the LLM for categorized suggestions
    let llm = get_llm_client_fast(db);
    let prompt = format!(
        "You are the NeuroVault sidekick. Below is what the user is currently working on, \
         plus knowledge the brain has from past projects. Generate up to 3 INSIGHTS, 3 WARNINGS, \
         and 3 OPTIMIZATIONS based ONLY on the context provided. Output in this exact format:\n\n\
         INSIGHT: <one sentence>\n\
         INSIGHT: <one sentence>\n\
         WARNING: <one sentence>\n\
         OPTIMIZATION: <one sentence>\n\n\
         Each line must start with INSIGHT:, WARNING:, or OPTIMIZATION:. \
         If a category has nothing, omit it. If nothing useful at all, output just NONE.\n\n\
         CURRENT WORK:\n{}\n\n\
         {}",
        short(&query, 600),
        context
    );

    let response = match llm.generate(&prompt, 500).await {
        Ok(r) => r,
        Err(e) => return Err(format!("llm: {}", e)),
    };

    if response.trim().to_uppercase().starts_with("NONE") || response.trim().is_empty() {
        return Ok(());
    }

    let mut insights = Vec::new();
    let mut warnings = Vec::new();
    let mut optimizations = Vec::new();
    for line in response.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("INSIGHT:").or_else(|| line.strip_prefix("Insight:")) {
            if rest.trim().len() > 5 {
                insights.push(rest.trim().to_string());
            }
        } else if let Some(rest) = line.strip_prefix("WARNING:").or_else(|| line.strip_prefix("Warning:")) {
            if rest.trim().len() > 5 {
                warnings.push(rest.trim().to_string());
            }
        } else if let Some(rest) = line.strip_prefix("OPTIMIZATION:").or_else(|| line.strip_prefix("Optimization:")) {
            if rest.trim().len() > 5 {
                optimizations.push(rest.trim().to_string());
            }
        }
    }

    if insights.is_empty() && warnings.is_empty() && optimizations.is_empty() {
        return Ok(());
    }

    let project = session
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();
    let session_name = session
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("session")
        .to_string();

    let suggestions = ActiveSuggestions {
        generated_at: chrono::Utc::now().to_rfc3339(),
        project: project.clone(),
        session: session_name.clone(),
        query_basis: short(&query, 200),
        insights,
        warnings,
        optimizations,
    };

    // Write the markdown file
    let md = render_markdown(&suggestions);
    let path = db.config.export_dir().join("active-suggestions.md");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&path, md).map_err(|e| format!("write {}: {}", path.display(), e))?;

    // Also write the JSON snapshot for the Tauri command
    let json_path = db.config.export_dir().join("active-suggestions.json");
    let json_str = serde_json::to_string_pretty(&suggestions).unwrap_or_default();
    std::fs::write(&json_path, json_str).map_err(|e| format!("write json: {}", e))?;

    log::info!(
        "Sidekick suggestions: {} insights, {} warnings, {} optimizations",
        suggestions.insights.len(),
        suggestions.warnings.len(),
        suggestions.optimizations.len()
    );
    Ok(())
}

fn render_markdown(s: &ActiveSuggestions) -> String {
    let mut md = String::new();
    md.push_str("# Active Suggestions — NeuroVault\n\n");
    md.push_str("> Auto-written by the brain's suggestion engine. ");
    md.push_str("Refreshed every 60 seconds while you're in a Claude Code session.\n\n");
    md.push_str(&format!("**Project:** `{}`\n", s.project));
    md.push_str(&format!("**Session:** `{}`\n", s.session));
    md.push_str(&format!("**Updated:** {}\n", s.generated_at));
    md.push_str(&format!("**Based on:** {}\n\n", s.query_basis));

    if !s.insights.is_empty() {
        md.push_str("## Insights\n\n");
        for i in &s.insights {
            md.push_str(&format!("- {}\n", i));
        }
        md.push('\n');
    }
    if !s.warnings.is_empty() {
        md.push_str("## Warnings\n\n");
        for w in &s.warnings {
            md.push_str(&format!("- {}\n", w));
        }
        md.push('\n');
    }
    if !s.optimizations.is_empty() {
        md.push_str("## Optimizations\n\n");
        for o in &s.optimizations {
            md.push_str(&format!("- {}\n", o));
        }
        md.push('\n');
    }
    md.push_str("---\n\n");
    md.push_str("_The brain compounds. Every conversation makes the next one smarter._\n");
    md
}

fn short(s: &str, max: usize) -> String {
    let s = s.trim().replace('\n', " ");
    if s.chars().count() <= max {
        s
    } else {
        let truncated: String = s.chars().take(max).collect();
        format!("{}...", truncated)
    }
}

/// Tauri command to read the most recent suggestions snapshot.
pub async fn load_active_suggestions(db: &BrainDb) -> Result<ActiveSuggestions, String> {
    let path = db.config.export_dir().join("active-suggestions.json");
    if !path.exists() {
        return Ok(ActiveSuggestions::default());
    }
    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&content).map_err(|e| e.to_string())
}
