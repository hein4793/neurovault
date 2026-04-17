//! Dream Mode — Phase 6 of the Dual-Brain plan.
//!
//! Overnight deep processing circuits that run when the system is idle.
//! Uses the DEEP LLM model (Qwen 32B) for multi-pass reasoning that would
//! be too expensive during active work hours.
//!
//! ## Circuits
//!
//! - `deep_synthesis` — 5-pass reasoning on the day's top nodes
//! - `morning_briefing` — compile overnight discoveries into a briefing

use crate::db::models::CreateNodeInput;
use crate::db::BrainDb;
use crate::error::BrainError;
use rusqlite::params;
use std::sync::Arc;

// =========================================================================
// Circuit: deep_synthesis (overnight deep reasoning)
// =========================================================================

pub async fn circuit_deep_synthesis(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    // Only run during idle hours (10 PM - 6 AM local time)
    let hour = chrono::Local::now().hour();
    if !(hour >= 22 || hour < 6) {
        return Ok("Skipping deep_synthesis — not in overnight window (22:00-06:00)".into());
    }

    // Get the day's top 10 most interesting nodes (high quality + recently created)
    let top_nodes: Vec<(String, String, String)> = db
        .with_conn(|conn| {
            let cutoff = (chrono::Utc::now() - chrono::Duration::hours(18)).to_rfc3339();
            let mut stmt = conn
                .prepare(
                    "SELECT id, title, content FROM nodes
                     WHERE created_at > ?1 AND quality_score > 0.5
                       AND node_type NOT IN ('session_summary', 'conversation')
                     ORDER BY quality_score DESC, access_count DESC
                     LIMIT 10",
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt
                .query_map(params![cutoff], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let mut result = Vec::new();
            for r in rows {
                if let Ok(v) = r {
                    result.push(v);
                }
            }
            Ok(result)
        })
        .await?;

    if top_nodes.is_empty() {
        return Ok("No high-quality nodes from today to synthesize".into());
    }

    // Build a corpus of today's best nodes
    let mut corpus = String::new();
    for (_, title, content) in &top_nodes {
        corpus.push_str(&format!("## {}\n{}\n\n", title, crate::truncate_str(content, 500)));
    }

    let prompt = format!(
        "You are performing deep synthesis on today's most important knowledge. \
         Analyze these nodes across multiple passes:\n\n\
         Pass 1: What are the core themes?\n\
         Pass 2: What connections exist between seemingly unrelated topics?\n\
         Pass 3: What novel insights emerge from combining these ideas?\n\
         Pass 4: What predictions can be made based on these patterns?\n\
         Pass 5: What is the single most important takeaway?\n\n\
         Be specific and concrete. Reference the actual topics discussed.\n\n\
         TODAY'S TOP NODES:\n{}", corpus
    );

    let llm = crate::commands::ai::get_llm_client_deep(db);
    let response = llm.generate(&prompt, 2000).await?;

    // Store as a dream synthesis node
    db.create_node(CreateNodeInput {
        title: format!("Dream Synthesis — {}", chrono::Local::now().format("%Y-%m-%d")),
        content: response.clone(),
        domain: "synthesis".to_string(),
        topic: "dream-synthesis".to_string(),
        tags: vec!["dream".to_string(), "synthesis".to_string(), "overnight".to_string(), "deep".to_string()],
        node_type: "insight".to_string(),
        source_type: "dream_mode".to_string(),
        source_url: None,
    }).await?;

    Ok(format!("Deep synthesis complete: {} nodes analyzed, insight created", top_nodes.len()))
}

// =========================================================================
// Circuit: morning_briefing (compile overnight discoveries)
// =========================================================================

pub async fn circuit_morning_briefing(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    // Only run in the morning (5 AM - 8 AM local time)
    let hour = chrono::Local::now().hour();
    if !(5..=8).contains(&hour) {
        return Ok("Skipping morning_briefing — not in morning window (05:00-08:00)".into());
    }

    // Check if we already wrote a briefing today
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let today_clone = today.clone();
    let already_done: bool = db
        .with_conn(move |conn| {
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM nodes
                     WHERE node_type = 'insight' AND topic = 'morning-briefing'
                       AND title LIKE '%' || ?1 || '%'",
                    params![today_clone],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            Ok(count > 0)
        })
        .await?;

    if already_done {
        return Ok("Morning briefing already generated today".into());
    }

    // Gather overnight circuit results
    let overnight_cutoff = (chrono::Utc::now() - chrono::Duration::hours(10)).to_rfc3339();
    let circuit_results: Vec<(String, String)> = db
        .with_conn(move |conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT circuit_name, result FROM autonomy_circuit_log
                     WHERE started_at > ?1 AND status = 'ok'
                       AND result != '' AND result NOT LIKE '%skip%'
                     ORDER BY started_at DESC LIMIT 20",
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt
                .query_map(params![overnight_cutoff], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let mut result = Vec::new();
            for r in rows {
                if let Ok(v) = r {
                    result.push(v);
                }
            }
            Ok(result)
        })
        .await?;

    // Gather new nodes created overnight
    let overnight_cutoff2 = (chrono::Utc::now() - chrono::Duration::hours(10)).to_rfc3339();
    let new_nodes: Vec<(String, String)> = db
        .with_conn(move |conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT title, domain FROM nodes
                     WHERE created_at > ?1 AND synthesized_by_brain = 1
                     ORDER BY quality_score DESC LIMIT 10",
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt
                .query_map(params![overnight_cutoff2], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let mut result = Vec::new();
            for r in rows {
                if let Ok(v) = r {
                    result.push(v);
                }
            }
            Ok(result)
        })
        .await?;

    // Build the briefing
    let mut briefing = format!("# Morning Briefing — {}\n\n", today);
    briefing.push_str("*What your brain did overnight while you slept.*\n\n");

    briefing.push_str("## Circuit Activity\n\n");
    if circuit_results.is_empty() {
        briefing.push_str("No circuit activity overnight.\n\n");
    } else {
        for (name, result) in &circuit_results {
            let short_result = if result.len() > 120 {
                format!("{}...", &result[..120])
            } else {
                result.clone()
            };
            briefing.push_str(&format!("- **{}**: {}\n", name, short_result));
        }
        briefing.push('\n');
    }

    briefing.push_str("## New Discoveries\n\n");
    if new_nodes.is_empty() {
        briefing.push_str("No new brain-generated nodes overnight.\n\n");
    } else {
        for (title, domain) in &new_nodes {
            briefing.push_str(&format!("- [{}] {}\n", domain, title));
        }
        briefing.push('\n');
    }

    // Write to export file
    let path = db.config.export_dir().join("morning-briefing.md");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&path, &briefing)?;

    // Also store as a node
    db.create_node(CreateNodeInput {
        title: format!("Morning Briefing — {}", today),
        content: briefing,
        domain: "meta".to_string(),
        topic: "morning-briefing".to_string(),
        tags: vec!["briefing".to_string(), "morning".to_string(), "overnight".to_string()],
        node_type: "insight".to_string(),
        source_type: "dream_mode".to_string(),
        source_url: None,
    }).await?;

    Ok(format!("Morning briefing generated: {} circuits, {} new nodes", circuit_results.len(), new_nodes.len()))
}

use chrono::Timelike;
