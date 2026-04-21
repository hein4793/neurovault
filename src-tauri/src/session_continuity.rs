//! Cross-Session Continuity — Phase 3 of the Dual-Brain plan.
//!
//! Extracts structured intelligence from completed Claude Code sessions
//! and produces session summaries that enable seamless handoff between
//! conversations. Claude never actually "remembers" — but it reads a
//! perfect summary of what happened, and from the user's perspective
//! it feels like continuity.
//!
//! ## Session Summarizer Circuit
//!
//! Runs as part of the autonomy rotation. On each cycle:
//! 1. Scans `~/.claude/projects/` for session files modified since last run
//! 2. For sessions that appear completed (no modification in >10 minutes),
//!    extracts structured intelligence via LLM
//! 3. Creates a `session_summary` node in the brain
//! 4. Writes the latest summary to `~/.neurovault/export/session-handoff.md`
//!    so the next Claude Code session can pick up where the last left off

use crate::db::models::CreateNodeInput;
use crate::db::BrainDb;
use crate::error::BrainError;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

// =========================================================================
// Session summary types
// =========================================================================

#[derive(Debug, Clone)]
pub struct SessionSummary {
    pub project: String,
    pub session_id: String,
    pub decisions: Vec<String>,
    pub code_written: Vec<String>,
    pub problems_solved: Vec<String>,
    pub open_questions: Vec<String>,
    pub next_steps: Vec<String>,
    pub key_files_touched: Vec<String>,
    pub duration_estimate: String,
    pub raw_summary: String,
}

// =========================================================================
// Circuit: session_summarizer
// =========================================================================

pub async fn circuit_session_summarizer(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    let home = dirs::home_dir().ok_or_else(|| BrainError::Ingestion("no home dir".into()))?;
    let projects_dir = home.join(".claude").join("projects");
    if !projects_dir.exists() {
        return Ok("No Claude projects directory found".into());
    }

    // Find sessions not modified in the last 10 minutes (likely completed)
    let cutoff = SystemTime::now() - Duration::from_secs(600);
    let sessions = find_completed_sessions(&projects_dir, cutoff);

    if sessions.is_empty() {
        return Ok("No completed sessions to summarize".into());
    }

    // Check which sessions we've already summarized
    let already_done: HashSet<String> = db
        .with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT source_url FROM nodes
                     WHERE node_type = 'session_summary'
                     ORDER BY created_at DESC LIMIT 200",
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let mut set = HashSet::new();
            for r in rows {
                if let Ok(v) = r {
                    set.insert(v);
                }
            }
            Ok(set)
        })
        .await?;

    let mut summarized = 0u32;
    let mut latest_summary: Option<SessionSummary> = None;

    for session_path in &sessions {
        let session_key = session_path.to_string_lossy().to_string();
        if already_done.contains(&session_key) {
            continue;
        }

        match summarize_session(db, session_path).await {
            Ok(summary) => {
                store_summary(db, &summary, &session_key).await?;
                latest_summary = Some(summary);
                summarized += 1;
            }
            Err(e) => {
                log::warn!("Session summarizer: failed on {}: {}", session_path.display(), e);
            }
        }

        if summarized >= 3 {
            break;
        }
    }

    // Write the handoff file for the next session
    if let Some(ref summary) = latest_summary {
        write_handoff_file(db, summary)?;
    }

    Ok(format!("Summarized {} sessions", summarized))
}

// =========================================================================
// Find completed session files
// =========================================================================

fn find_completed_sessions(projects_dir: &Path, modified_before: SystemTime) -> Vec<PathBuf> {
    let mut sessions = Vec::new();
    let entries = match std::fs::read_dir(projects_dir) {
        Ok(e) => e,
        Err(_) => return sessions,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        // Each project dir contains .jsonl session files
        if let Ok(files) = std::fs::read_dir(&path) {
            for file in files.flatten() {
                let fp = file.path();
                if fp.extension().and_then(|s| s.to_str()) != Some("jsonl") {
                    continue;
                }
                if fp.to_string_lossy().contains("subagents") {
                    continue;
                }
                if let Ok(meta) = file.metadata() {
                    if let Ok(mtime) = meta.modified() {
                        if mtime < modified_before && meta.len() > 500 {
                            sessions.push(fp);
                        }
                    }
                }
            }
        }
    }

    // Sort newest first
    sessions.sort_by(|a, b| {
        let ma = std::fs::metadata(a).and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
        let mb = std::fs::metadata(b).and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
        mb.cmp(&ma)
    });
    sessions.truncate(10);
    sessions
}

// =========================================================================
// Extract structured summary from a session file via LLM
// =========================================================================

async fn summarize_session(
    db: &Arc<BrainDb>,
    session_path: &Path,
) -> Result<SessionSummary, BrainError> {
    let content = std::fs::read_to_string(session_path)?;
    let lines: Vec<&str> = content.lines().collect();

    // Extract the last 40 messages (captures the meaty part of the session)
    let start = if lines.len() > 40 { lines.len() - 40 } else { 0 };
    let mut conversation = String::new();
    for line in &lines[start..] {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
            let msg_type = json["type"].as_str().unwrap_or("");
            if msg_type != "human" && msg_type != "assistant" {
                continue;
            }
            let role = if msg_type == "human" { "USER" } else { "ASSISTANT" };
            if let Some(content_arr) = json["message"]["content"].as_array() {
                for item in content_arr {
                    if let Some(text) = item["text"].as_str() {
                        if text.len() > 20 {
                            let trimmed = crate::truncate_str(text, 400);
                            conversation.push_str(&format!("{}: {}\n", role, trimmed));
                        }
                    }
                }
            }
        }
    }

    if conversation.len() < 100 {
        return Err(BrainError::Ingestion("Session too short to summarize".into()));
    }

    let project = session_path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .map(crate::decode_claude_project_name)
        .unwrap_or_else(|| "unknown".to_string());

    let session_id = session_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    let prompt = format!(
        "Analyze this Claude Code session and extract structured intelligence.\n\
         Output EXACTLY this format (keep each section, use - bullets, leave empty sections blank):\n\n\
         DECISIONS:\n- <decision made and why>\n\n\
         CODE_WRITTEN:\n- <what was implemented or changed>\n\n\
         PROBLEMS_SOLVED:\n- <what bugs/issues were fixed>\n\n\
         OPEN_QUESTIONS:\n- <unresolved questions or TODOs>\n\n\
         NEXT_STEPS:\n- <what should happen next>\n\n\
         KEY_FILES:\n- <important file paths touched>\n\n\
         DURATION: <estimated session length>\n\n\
         SUMMARY: <2-3 sentence overview>\n\n\
         SESSION:\n{}", crate::truncate_str(&conversation, 6000)
    );

    let llm = crate::commands::ai::get_llm_client_fast(db);
    let response = llm.generate(&prompt, 1500).await?;

    Ok(parse_session_summary(&response, &project, &session_id))
}

fn parse_session_summary(response: &str, project: &str, session_id: &str) -> SessionSummary {
    let mut summary = SessionSummary {
        project: project.to_string(),
        session_id: session_id.to_string(),
        decisions: Vec::new(),
        code_written: Vec::new(),
        problems_solved: Vec::new(),
        open_questions: Vec::new(),
        next_steps: Vec::new(),
        key_files_touched: Vec::new(),
        duration_estimate: String::new(),
        raw_summary: String::new(),
    };

    let mut current_section = "";
    for line in response.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        match trimmed {
            "DECISIONS:" => { current_section = "decisions"; continue; }
            "CODE_WRITTEN:" => { current_section = "code"; continue; }
            "PROBLEMS_SOLVED:" => { current_section = "problems"; continue; }
            "OPEN_QUESTIONS:" => { current_section = "questions"; continue; }
            "NEXT_STEPS:" => { current_section = "next"; continue; }
            "KEY_FILES:" => { current_section = "files"; continue; }
            _ => {}
        }

        if trimmed.starts_with("DURATION:") {
            summary.duration_estimate = trimmed.trim_start_matches("DURATION:").trim().to_string();
            current_section = "";
            continue;
        }
        if trimmed.starts_with("SUMMARY:") {
            summary.raw_summary = trimmed.trim_start_matches("SUMMARY:").trim().to_string();
            current_section = "summary_cont";
            continue;
        }

        let item = trimmed.trim_start_matches("- ").to_string();
        if item.is_empty() {
            continue;
        }

        match current_section {
            "decisions" => summary.decisions.push(item),
            "code" => summary.code_written.push(item),
            "problems" => summary.problems_solved.push(item),
            "questions" => summary.open_questions.push(item),
            "next" => summary.next_steps.push(item),
            "files" => summary.key_files_touched.push(item),
            "summary_cont" => {
                summary.raw_summary.push(' ');
                summary.raw_summary.push_str(trimmed);
            }
            _ => {}
        }
    }

    summary
}

// =========================================================================
// Store summary as a node
// =========================================================================

async fn store_summary(
    db: &Arc<BrainDb>,
    summary: &SessionSummary,
    session_key: &str,
) -> Result<(), BrainError> {
    let mut content = format!("# Session Summary: {}\n\n", summary.project);
    content.push_str(&format!("**Session:** {}\n", summary.session_id));
    content.push_str(&format!("**Duration:** {}\n\n", summary.duration_estimate));

    if !summary.decisions.is_empty() {
        content.push_str("## Decisions Made\n");
        for d in &summary.decisions { content.push_str(&format!("- {}\n", d)); }
        content.push('\n');
    }
    if !summary.code_written.is_empty() {
        content.push_str("## Code Written\n");
        for c in &summary.code_written { content.push_str(&format!("- {}\n", c)); }
        content.push('\n');
    }
    if !summary.problems_solved.is_empty() {
        content.push_str("## Problems Solved\n");
        for p in &summary.problems_solved { content.push_str(&format!("- {}\n", p)); }
        content.push('\n');
    }
    if !summary.open_questions.is_empty() {
        content.push_str("## Open Questions\n");
        for q in &summary.open_questions { content.push_str(&format!("- {}\n", q)); }
        content.push('\n');
    }
    if !summary.next_steps.is_empty() {
        content.push_str("## Next Steps\n");
        for n in &summary.next_steps { content.push_str(&format!("- {}\n", n)); }
        content.push('\n');
    }
    if !summary.key_files_touched.is_empty() {
        content.push_str("## Key Files\n");
        for f in &summary.key_files_touched { content.push_str(&format!("- {}\n", f)); }
        content.push('\n');
    }

    // Use create_node for proper dedup + indexing (handles content_hash internally)
    db.create_node(CreateNodeInput {
        title: format!("Session: {} — {}", summary.project, &summary.session_id[..8.min(summary.session_id.len())]),
        content,
        domain: "personal".to_string(),
        topic: summary.project.to_lowercase().replace(' ', "-"),
        tags: vec!["session_summary".to_string(), "handoff".to_string(), "auto".to_string()],
        node_type: "session_summary".to_string(),
        source_type: "session_summarizer".to_string(),
        source_url: Some(session_key.to_string()),
    }).await?;

    Ok(())
}

// =========================================================================
// Write handoff file for next session
// =========================================================================

fn write_handoff_file(db: &BrainDb, summary: &SessionSummary) -> Result<(), BrainError> {
    let path = db.config.export_dir().join("session-handoff.md");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let now = chrono::Utc::now().to_rfc3339();
    let mut md = String::new();
    md.push_str("# Last Session Handoff\n\n");
    md.push_str(&format!("*Auto-generated by session_summarizer circuit. Updated: {}*\n\n", now));
    md.push_str(&format!("**Project:** {}\n", summary.project));
    md.push_str(&format!("**Duration:** {}\n\n", summary.duration_estimate));

    if !summary.raw_summary.is_empty() {
        md.push_str(&format!("**Summary:** {}\n\n", summary.raw_summary));
    }

    md.push_str("## In our last session, we:\n\n");
    for d in &summary.decisions {
        md.push_str(&format!("- Decided: {}\n", d));
    }
    for c in &summary.code_written {
        md.push_str(&format!("- Implemented: {}\n", c));
    }
    for p in &summary.problems_solved {
        md.push_str(&format!("- Fixed: {}\n", p));
    }
    md.push('\n');

    if !summary.open_questions.is_empty() {
        md.push_str("## Still open:\n\n");
        for q in &summary.open_questions {
            md.push_str(&format!("- {}\n", q));
        }
        md.push('\n');
    }

    if !summary.next_steps.is_empty() {
        md.push_str("## Next steps:\n\n");
        for n in &summary.next_steps {
            md.push_str(&format!("- {}\n", n));
        }
        md.push('\n');
    }

    md.push_str("---\n");
    md.push_str("_Read this at the start of your next session. Claude picks up exactly where you left off._\n");

    std::fs::write(&path, md)?;
    log::info!("Session handoff written to {}", path.display());
    Ok(())
}
