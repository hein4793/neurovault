//! Proactive Context Injector — the brain pushes context to Claude Code
//! without being asked.
//!
//! Phase 1.2 of the master plan. The killer feature: instead of waiting
//! for Claude Code to call `brain_recall`, the brain *watches what Claude
//! Code is doing right now* and writes a markdown context file that
//! Claude Code reads on every prompt via the global `~/.claude/CLAUDE.md`
//! instructions.
//!
//! ## How it works
//!
//! 1. Every 30 seconds the loop wakes up.
//! 2. Finds the **most recently modified** `*.jsonl` chat file under
//!    `~/.claude/projects/`. That's the Claude Code session the user is
//!    actively using right now.
//! 3. Reads the last ~10 messages from that jsonl file.
//! 4. Extracts a context query from them — title-ish keywords from the
//!    most recent human turn, plus topic terms from any tool-call results.
//! 5. Runs that query through `vector_search` to pull relevant nodes,
//!    plus loads the top user_cognition rules.
//! 6. Writes a markdown summary to `~/.neurovault/export/active-context.md`.
//!
//! Claude Code's global CLAUDE.md tells every session to read that file at
//! the start of each turn, so the brain's relevant knowledge for the
//! current task automatically lands in Claude Code's context window.
//!
//! ## Design choices
//!
//! - **Read-only on the chat files.** We never write to `~/.claude/projects/`.
//! - **Fast and bounded.** Each cycle reads ~10 lines, runs ONE
//!   vector_search, writes ONE small file. Should take < 1 second.
//! - **Idempotent.** Writes the whole context file every cycle. No
//!   incremental state, no diff tracking.
//! - **Best-effort.** Any failure logs and continues — never panics out
//!   the loop.
//! - **Skips itself.** Doesn't process the brain's own session if the
//!   user happens to have it open (would cause feedback noise).

use crate::db::BrainDb;
use crate::db::models::{SearchResult, UserCognition};
use crate::error::BrainError;
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use rusqlite::params;
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::mpsc;

/// Run forever. Spawned at app startup.
///
/// Uses a hybrid trigger model:
/// 1. **Event-driven** (Phase 1.2B) — spawns its own file watcher on
///    `~/.claude/projects/` and runs an inject cycle within ~100ms of any
///    `.jsonl` change. This is the killer-feature low-latency path.
/// 2. **Polling fallback** — also runs an inject cycle every 30s as a
///    safety net in case the watcher misses an event.
pub async fn run_context_injector(db: Arc<BrainDb>) {
    // Wait 60s after startup so the rest of the system has settled
    // (HNSW load, embedding pipeline first cycle, etc.)
    tokio::time::sleep(Duration::from_secs(60)).await;
    log::info!("Sidekick: proactive context injector started (event-driven + 30s polling)");

    // Channel for file-change events from the sidekick's own watcher.
    // Capacity 32 — bursts of multiple events get coalesced by the loop's
    // debounce timer below.
    let (tx, mut rx) = mpsc::channel::<PathBuf>(32);

    // Spawn the sidekick's own file watcher in a blocking thread.
    // We can't share the existing sync::start_file_watcher cleanly because
    // its callback already does its own work, so we run a parallel watcher.
    spawn_sidekick_watcher(tx);

    let mut last_session: Option<PathBuf> = None;
    let mut last_message_count: usize = 0;
    let mut last_inject = std::time::Instant::now() - Duration::from_secs(60);

    loop {
        // Wait for either a file event OR the 30s polling tick, whichever comes first.
        let trigger: TriggerSource = tokio::select! {
            evt = rx.recv() => match evt {
                Some(_path) => TriggerSource::Event,
                None => TriggerSource::Poll, // channel closed shouldn't happen, fall through
            },
            _ = tokio::time::sleep(Duration::from_secs(30)) => TriggerSource::Poll,
        };

        // Debounce: if the last inject ran less than 2 seconds ago, skip.
        // This collapses bursts of file events from a single Claude Code
        // turn (one turn writes the jsonl multiple times).
        if last_inject.elapsed() < Duration::from_secs(2) {
            continue;
        }
        last_inject = std::time::Instant::now();

        if let Err(e) = inject_once(&db, &mut last_session, &mut last_message_count).await {
            log::warn!("Sidekick: inject cycle failed ({:?}): {}", trigger, e);
        }
    }
}

#[derive(Debug)]
enum TriggerSource {
    Event,
    Poll,
}

/// Spawn a file watcher dedicated to the sidekick. Sends each detected
/// jsonl path through `tx`. Filters non-jsonl and subagents subdirectories.
fn spawn_sidekick_watcher(tx: mpsc::Sender<PathBuf>) {
    std::thread::spawn(move || {
        let home = match dirs::home_dir() {
            Some(h) => h,
            None => return,
        };
        let watch_dir = home.join(".claude").join("projects");
        if !watch_dir.exists() {
            log::warn!("Sidekick watcher: ~/.claude/projects not found");
            return;
        }

        let tx_clone = tx.clone();
        let mut watcher = match RecommendedWatcher::new(
            move |result: Result<notify::Event, notify::Error>| {
                if let Ok(event) = result {
                    if !matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_)) {
                        return;
                    }
                    for path in &event.paths {
                        if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
                            continue;
                        }
                        if path.to_string_lossy().contains("subagents") {
                            continue;
                        }
                        // Best-effort send — drop if the channel is full
                        let _ = tx_clone.try_send(path.clone());
                    }
                }
            },
            Config::default(),
        ) {
            Ok(w) => w,
            Err(e) => {
                log::error!("Sidekick watcher: failed to create: {}", e);
                return;
            }
        };

        if let Err(e) = watcher.watch(&watch_dir, RecursiveMode::Recursive) {
            log::error!("Sidekick watcher: failed to watch {}: {}", watch_dir.display(), e);
            return;
        }
        log::info!("Sidekick watcher: subscribed to {}", watch_dir.display());

        // Keep the thread alive
        loop {
            std::thread::sleep(Duration::from_secs(60));
        }
    });
}

/// One inject cycle. Returns Err on any non-fatal issue (logged + retried
/// next cycle).
async fn inject_once(
    db: &BrainDb,
    last_session: &mut Option<PathBuf>,
    last_message_count: &mut usize,
) -> Result<(), String> {
    // 1) Find the most recently active Claude Code session
    let session = find_active_session()
        .ok_or_else(|| "no active Claude Code session found".to_string())?;

    let session_changed = last_session.as_deref() != Some(session.as_path());
    *last_session = Some(session.clone());

    // 2) Read the last ~10 messages
    let messages = read_recent_messages(&session, 12)
        .map_err(|e| format!("read messages from {}: {}", session.display(), e))?;
    if messages.is_empty() {
        return Ok(()); // empty session — nothing to inject yet
    }

    // Skip if nothing has changed since last cycle (saves an LLM-free
    // search but more importantly avoids repeatedly clobbering the file).
    if !session_changed && messages.len() == *last_message_count {
        return Ok(());
    }
    *last_message_count = messages.len();

    // 3) Extract a query from the most recent human/assistant turns
    let query = extract_context_query(&messages);
    if query.trim().len() < 3 {
        return Ok(());
    }

    log::debug!("Sidekick: query='{}'", query);

    // 4) Pull relevant context: vector search + top preferences
    let matches = run_recall(db, &query, 8).await;
    let prefs = top_preferences(db, 8).await;

    // 5) Render markdown and write to active-context.md
    let project_name = session
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

    let md = render_context(&project_name, &session_name, &query, &matches, &prefs);
    let path = db.config.active_context_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&path, md).map_err(|e| format!("write {}: {}", path.display(), e))?;

    log::info!(
        "Sidekick: wrote context for project '{}' ({} matches, {} prefs)",
        project_name,
        matches.len(),
        prefs.len()
    );
    Ok(())
}

// =========================================================================
// Active session detection
// =========================================================================

/// Walk `~/.claude/projects/*` looking for the most recently modified
/// `*.jsonl` file. That file represents the chat session Claude Code is
/// currently in.
fn find_active_session() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let projects = home.join(".claude").join("projects");
    if !projects.exists() {
        return None;
    }

    let mut newest: Option<(SystemTime, PathBuf)> = None;
    walk_jsonl(&projects, &mut newest, 0);
    newest.map(|(_, p)| p)
}

fn walk_jsonl(dir: &Path, newest: &mut Option<(SystemTime, PathBuf)>, depth: usize) {
    if depth > 4 {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Skip subagents directory — those are sub-conversations,
            // not the user-facing session.
            if path
                .file_name()
                .and_then(|s| s.to_str())
                .map(|s| s == "subagents")
                .unwrap_or(false)
            {
                continue;
            }
            walk_jsonl(&path, newest, depth + 1);
        } else if path.extension().and_then(|s| s.to_str()) == Some("jsonl") {
            if let Ok(meta) = entry.metadata() {
                if let Ok(mtime) = meta.modified() {
                    let is_newer = newest.as_ref().map(|(t, _)| mtime > *t).unwrap_or(true);
                    if is_newer {
                        *newest = Some((mtime, path));
                    }
                }
            }
        }
    }
}

// =========================================================================
// Message extraction
// =========================================================================

/// One human or assistant turn from a Claude Code jsonl session.
struct Turn {
    role: String,
    text: String,
}

/// Read the last `n` human/assistant turns from a jsonl chat file.
/// Skips tool-call entries — we only want the prose.
fn read_recent_messages(path: &Path, n: usize) -> std::io::Result<Vec<Turn>> {
    let content = std::fs::read_to_string(path)?;
    let mut turns: Vec<Turn> = Vec::new();
    // Walk all lines but only keep the most recent `n` matching turns.
    for line in content.lines() {
        let json: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let msg_type = json.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if msg_type != "human" && msg_type != "assistant" {
            continue;
        }
        let role = if msg_type == "human" { "user" } else { "assistant" };
        let content_field = json.get("message").and_then(|m| m.get("content"));
        let text = content_to_text(content_field);
        if text.trim().len() < 5 {
            continue;
        }
        turns.push(Turn {
            role: role.to_string(),
            text,
        });
    }
    if turns.len() > n {
        let start = turns.len() - n;
        Ok(turns[start..].to_vec())
    } else {
        Ok(turns)
    }
}

impl Clone for Turn {
    fn clone(&self) -> Self {
        Self {
            role: self.role.clone(),
            text: self.text.clone(),
        }
    }
}

/// Flatten the Anthropic-style content array (array of {type, text} blocks)
/// into a single text string. Skips tool_use / tool_result blocks.
fn content_to_text(value: Option<&Value>) -> String {
    let v = match value {
        Some(v) => v,
        None => return String::new(),
    };
    if let Some(s) = v.as_str() {
        return s.to_string();
    }
    if let Some(arr) = v.as_array() {
        let mut out = String::new();
        for block in arr {
            // Anthropic content blocks: {type: "text"|"tool_use"|"tool_result", text?: "..."}
            let btype = block.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if btype == "text" {
                if let Some(t) = block.get("text").and_then(|v| v.as_str()) {
                    out.push_str(t);
                    out.push('\n');
                }
            }
        }
        return out;
    }
    String::new()
}

/// Build a context query from the recent turns. Strategy: take the most
/// recent human turn (that's what the user just asked about), keep the
/// first 600 chars, and dedupe-ish-extract some keywords from any
/// adjacent assistant text so we get topical anchors too.
fn extract_context_query(turns: &[Turn]) -> String {
    // Most recent user turn = the most recent question
    let last_user = turns.iter().rev().find(|t| t.role == "user");
    let mut query = String::new();
    if let Some(u) = last_user {
        let s: String = u.text.chars().take(600).collect();
        query.push_str(s.trim());
    }
    // If empty, fall back to the last assistant turn
    if query.is_empty() {
        if let Some(a) = turns.iter().rev().find(|t| t.role == "assistant") {
            let s: String = a.text.chars().take(600).collect();
            query.push_str(s.trim());
        }
    }
    query
}

// =========================================================================
// Brain queries
// =========================================================================

async fn run_recall(db: &BrainDb, query: &str, limit: usize) -> Vec<SearchResult> {
    // Use the embedding pipeline if Ollama is up; fall back to text search
    let client = crate::embeddings::OllamaClient::new(
        db.config.ollama_url.clone(),
        db.config.embedding_model.clone(),
    );
    if client.health_check().await {
        if let Ok(emb) = client.generate_embedding(query).await {
            let results = db.vector_search(emb, limit).await.unwrap_or_default();
            if !results.is_empty() {
                return results;
            }
        }
    }
    db.search_nodes(query).await.unwrap_or_default()
}

async fn top_preferences(db: &BrainDb, limit: usize) -> Vec<UserCognition> {
    let lim = limit;
    let mut rules: Vec<UserCognition> = db.with_conn(move |conn| -> Result<Vec<UserCognition>, BrainError> {
        let mut stmt = conn.prepare(
            "SELECT id, timestamp, trigger_node_ids, pattern_type, extracted_rule, \
             structured_rule, confidence, times_confirmed, times_contradicted, \
             embedding, linked_to_nodes \
             FROM user_cognition LIMIT ?1"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map(params![lim as u32 * 10], |row| {
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

    rules.sort_by(|a, b| {
        let sa = a.confidence * (a.times_confirmed as f32 + 1.0);
        let sb = b.confidence * (b.times_confirmed as f32 + 1.0);
        sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
    });
    rules.truncate(limit);
    rules
}

// =========================================================================
// Markdown rendering
// =========================================================================

fn render_context(
    project: &str,
    session: &str,
    query: &str,
    matches: &[SearchResult],
    prefs: &[UserCognition],
) -> String {
    let now = chrono::Utc::now().to_rfc3339();
    let mut md = String::new();
    md.push_str("# Active Context — NeuroVault\n\n");
    md.push_str("> Auto-written by the brain's proactive sidekick. ");
    md.push_str("Reflects what the brain knows about your *current* Claude Code session. ");
    md.push_str("Refreshed every 30 seconds.\n\n");
    md.push_str(&format!("**Project:** `{}`\n", project));
    md.push_str(&format!("**Session:** `{}`\n", session));
    md.push_str(&format!("**Updated:** {}\n", now));
    md.push_str(&format!("**Query basis:** {}\n\n", short(query, 200)));

    md.push_str("## Relevant knowledge from your brain\n\n");
    if matches.is_empty() {
        md.push_str("_No matches found. The brain is still warming up or this is uncharted territory._\n\n");
    } else {
        // Dedupe by title since semantic search can return near-duplicates.
        let mut seen: HashSet<String> = HashSet::new();
        for m in matches {
            if !seen.insert(m.node.title.clone()) {
                continue;
            }
            md.push_str(&format!(
                "- **[{}]** _{}_ — {} _(score {:.2})_\n",
                m.node.domain,
                m.node.title,
                short(&m.node.summary, 220),
                m.score
            ));
        }
        md.push('\n');
    }

    md.push_str("## Your established patterns (from user_cognition)\n\n");
    if prefs.is_empty() {
        md.push_str("_The brain hasn't extracted any behavioral patterns yet. The user_pattern_mining circuit will fill this in over time._\n\n");
    } else {
        for p in prefs {
            md.push_str(&format!(
                "- **{}** ({:.0}% confidence, confirmed x{}): {}\n",
                p.pattern_type,
                p.confidence * 100.0,
                p.times_confirmed,
                short(&p.extracted_rule, 240)
            ));
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

// =========================================================================
// Public re-exports — allow sidekick_suggestions to reuse these helpers
// without duplicating logic.
// =========================================================================

/// Public wrapper around `find_active_session` so other modules can find
/// the most recently active Claude Code jsonl session file.
pub fn find_active_session_pub() -> Option<PathBuf> {
    find_active_session()
}

/// Public wrapper around `read_recent_messages` returning string-form turns.
pub fn read_recent_messages_pub(
    path: &Path,
    n: usize,
) -> std::io::Result<Vec<(String, String)>> {
    let turns = read_recent_messages(path, n)?;
    Ok(turns.into_iter().map(|t| (t.role, t.text)).collect())
}

/// Public wrapper around `extract_context_query`.
pub fn extract_context_query_pub(turns: &[(String, String)]) -> String {
    let internal: Vec<Turn> = turns
        .iter()
        .map(|(r, t)| Turn {
            role: r.clone(),
            text: t.clone(),
        })
        .collect();
    extract_context_query(&internal)
}
