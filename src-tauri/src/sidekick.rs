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

use crate::context_bundle::{build_context_bundle, render_sidekick_context};
use crate::db::BrainDb;
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde_json::Value;
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

    // 4) Build the 6-layer context bundle (rules, knowledge, patterns,
    //    decisions, warnings, predictions — all MMR-selected and compressed)
    let project_name = session
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    let bundle = build_context_bundle(db, &query, &project_name).await;

    // 5) Render to structured markdown and write
    let md = render_sidekick_context(&bundle);
    let path = db.config.active_context_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&path, &md).map_err(|e| format!("write {}: {}", path.display(), e))?;

    log::info!(
        "Sidekick: wrote 6-layer context for '{}' ({} rules, {} nodes, {} patterns, {} decisions, {} warnings, {} predictions — {}ms, ~{} chars)",
        project_name,
        bundle.compiled_rules.len(),
        bundle.knowledge_nodes.len(),
        bundle.work_patterns.len(),
        bundle.decisions.len(),
        bundle.warnings.len(),
        bundle.predictions.len(),
        bundle.generation_ms,
        bundle.total_chars,
    );

    // Phase 4: Log bundle quality for the optimizer circuit
    let _ = crate::context_quality::log_bundle_quality(
        db, &project_name, &query,
        bundle.compiled_rules.len(),
        bundle.knowledge_nodes.len(),
        bundle.work_patterns.len(),
        bundle.generation_ms,
        bundle.total_chars,
    ).await;

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

// Old render_context / run_recall / top_preferences removed — replaced by
// context_bundle::build_context_bundle + render_sidekick_context (Phase 1
// of the Dual-Brain plan).

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
