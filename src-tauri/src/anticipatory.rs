//! Anticipatory Context Loading — Phase 5 of the Dual-Brain plan.
//!
//! Pre-builds context bundles for predicted next tasks based on:
//! - **Project**: what project is active → preload architecture + recent decisions
//! - **Time patterns**: morning = briefing, afternoon = deep work context
//! - **Sequence patterns**: after test file → testing patterns, after error → solutions
//!
//! Runs every 15 minutes and writes precomputed context to
//! `~/.neurovault/export/anticipatory-context.md`.

use crate::context_bundle::{build_context_bundle, render_sidekick_context};
use crate::db::BrainDb;
use crate::error::BrainError;
use rusqlite::params;
use std::sync::Arc;

pub async fn circuit_anticipatory_preloader(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    // 1. Determine what the user is likely working on next
    let predictions = predict_next_context(db).await;

    if predictions.is_empty() {
        return Ok("No anticipatory predictions available".into());
    }

    // 2. Pre-build context bundles for top predictions
    let mut preloaded = 0u32;
    let mut md = String::new();
    md.push_str("# Anticipatory Context\n\n");
    md.push_str(&format!(
        "*Pre-built by the anticipatory preloader. Updated: {}*\n\n",
        chrono::Utc::now().to_rfc3339()
    ));

    for (query, reason) in &predictions {
        let bundle = build_context_bundle(db, query, "anticipatory").await;
        if bundle.knowledge_nodes.is_empty() && bundle.compiled_rules.is_empty() {
            continue;
        }

        md.push_str(&format!("## Predicted: {} ({})\n\n", query, reason));
        let section = render_sidekick_context(&bundle);
        // Extract just the content sections (skip the header)
        for line in section.lines().skip(3) {
            md.push_str(line);
            md.push('\n');
        }
        md.push_str("---\n\n");
        preloaded += 1;

        if preloaded >= 2 {
            break;
        }
    }

    // 3. Write anticipatory context file
    if preloaded > 0 {
        let path = db.config.export_dir().join("anticipatory-context.md");
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&path, &md);
    }

    Ok(format!("Pre-loaded {} anticipatory context bundles", preloaded))
}

async fn predict_next_context(db: &BrainDb) -> Vec<(String, String)> {
    let mut predictions: Vec<(String, String)> = Vec::new();

    // Strategy 1: Most recently active project topics
    let recent_topics: Vec<String> = db
        .with_conn(|conn| {
            let cutoff = (chrono::Utc::now() - chrono::Duration::hours(6)).to_rfc3339();
            let mut stmt = conn
                .prepare(
                    "SELECT DISTINCT topic FROM nodes
                     WHERE accessed_at > ?1 AND topic IS NOT NULL AND topic != ''
                     ORDER BY accessed_at DESC LIMIT 3",
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt
                .query_map(params![cutoff], |row| row.get::<_, String>(0))
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let mut result = Vec::new();
            for r in rows {
                if let Ok(v) = r {
                    result.push(v);
                }
            }
            Ok(result)
        })
        .await
        .unwrap_or_default();

    for topic in &recent_topics {
        predictions.push((topic.clone(), "recently active topic".into()));
    }

    // Strategy 2: Time-based patterns
    let hour = chrono::Local::now().hour();
    match hour {
        6..=9 => predictions.push(("morning briefing productivity plan".into(), "morning routine".into())),
        12..=13 => predictions.push(("afternoon deep work architecture".into(), "post-lunch deep work".into())),
        17..=19 => predictions.push(("evening review progress summary".into(), "end of day review".into())),
        _ => {}
    }

    // Strategy 3: Unresolved research missions
    let missions: Vec<String> = db
        .with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT topic FROM research_missions
                     WHERE status = 'pending'
                     ORDER BY created_at DESC LIMIT 2",
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let mut result = Vec::new();
            for r in rows {
                if let Ok(v) = r {
                    result.push(v);
                }
            }
            Ok(result)
        })
        .await
        .unwrap_or_default();

    for mission in &missions {
        predictions.push((mission.clone(), "pending research mission".into()));
    }

    predictions.truncate(4);
    predictions
}

use chrono::Timelike;
