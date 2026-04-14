//! Master Cognitive Loop — the Observe -> Analyze -> Improve -> Act cycle.
//!
//! Phase 2.1 of the master plan. While the 15 rotating circuits each
//! perform one bounded improvement task per 20-minute slot, this loop is
//! the **higher-order monitor** that watches what's happening across the
//! whole system and steers it. It runs every 30 minutes as its own
//! background task — separate from the circuit dispatcher.
//!
//! ## The four phases
//!
//! Each cycle runs four phases sequentially:
//!
//! ### OBSERVE
//! Pulls recent activity signals from across the brain:
//! - Last 20 entries from `autonomy_circuit_log` (what circuits ran, success/failure)
//! - Last 20 entries from `mcp_call_log` (what your AI assistant asked about)
//! - Recent node creation rate per source_type
//! - Recent thinking-node generation rate
//!
//! ### ANALYZE
//! Identifies patterns and inefficiencies:
//! - Are any circuits failing repeatedly? (-> flag for investigation)
//! - What topics is your AI assistant asking about most often? (-> research targets)
//! - Are thinking nodes being generated, or is the brain just storing more facts?
//! - Are quality / decay scores improving or degrading?
//!
//! ### IMPROVE
//! Acts on the analysis. The current implementation is conservative — it
//! creates an `insight` thinking node summarizing the cycle and queues
//! research missions for hot your AI assistant topics that the brain doesn't
//! cover yet. Future iterations could rebalance circuit weights, archive
//! stale nodes, or trigger targeted re-summarization.
//!
//! ### ACT
//! Persists the cycle's findings to a new `master_loop_log` table so
//! future cycles can compare trends, and writes the meta-insight node so
//! both the user (via the Insights panel) and your AI assistant (via
//! brain_recall) can see what the brain noticed about itself.

use crate::commands::ai::get_llm_client_fast;
use crate::db::models::{CreateNodeInput, NODE_TYPE_INSIGHT};
use crate::db::BrainDb;
use crate::error::BrainError;
use rusqlite::params;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

/// Spawn the master loop forever. Called from lib.rs setup().
pub async fn run_master_loop(db: Arc<BrainDb>) {
    // Wait 10 minutes after startup so the rest of the system stabilises
    // (HNSW build, embedding pipeline, autonomy first cycle).
    tokio::time::sleep(Duration::from_secs(600)).await;
    log::info!("Master cognitive loop started — observe -> analyze -> improve -> act, every 30 min");

    loop {
        match run_one_cycle(&db).await {
            Ok(summary) => log::info!("Master loop cycle complete: {}", summary),
            Err(e) => log::warn!("Master loop cycle failed: {}", e),
        }
        tokio::time::sleep(Duration::from_secs(1800)).await; // 30 minutes
    }
}

/// One full Observe -> Analyze -> Improve -> Act pass. Returns a one-line
/// summary of what happened.
async fn run_one_cycle(db: &BrainDb) -> Result<String, String> {
    let started_at = chrono::Utc::now().to_rfc3339();
    let cycle_start = std::time::Instant::now();

    // ===== OBSERVE =====
    let observation = observe(db).await.map_err(|e| format!("observe: {}", e))?;

    // ===== ANALYZE =====
    let analysis = analyze(&observation);

    // ===== IMPROVE =====
    let improvement = improve(db, &observation, &analysis)
        .await
        .map_err(|e| format!("improve: {}", e))?;

    // ===== ACT =====
    act(db, &observation, &analysis, &improvement, &started_at)
        .await
        .map_err(|e| format!("act: {}", e))?;

    let dur = cycle_start.elapsed().as_millis();
    Ok(format!(
        "obs={} circuits, {} mcp_calls, {} new_nodes ({}ms total) -> {}",
        observation.recent_circuits.len(),
        observation.recent_mcp_calls.len(),
        observation.new_nodes_24h,
        dur,
        improvement.summary
    ))
}

// =========================================================================
// OBSERVE
// =========================================================================

#[derive(Debug, Default)]
struct Observation {
    recent_circuits: Vec<CircuitRow>,
    recent_mcp_calls: Vec<McpCallRow>,
    new_nodes_24h: u64,
    new_thinking_nodes_24h: u64,
    /// Top 5 (command, count) from mcp_call_log in the last 24h
    top_mcp_commands: Vec<(String, u64)>,
    /// (circuit_name, success_count, fail_count) for the last 30 cycles
    circuit_health: Vec<(String, u64, u64)>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct CircuitRow {
    circuit_name: String,
    status: String,
    result: String,
    duration_ms: u64,
    started_at: String,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct McpCallRow {
    command: String,
    payload: String,
    called_at: String,
}

async fn observe(db: &BrainDb) -> Result<Observation, String> {
    let mut obs = Observation::default();

    // Last 30 circuit runs
    obs.recent_circuits = db.with_conn(|conn| -> Result<Vec<CircuitRow>, BrainError> {
        let mut stmt = conn.prepare(
            "SELECT circuit_name, status, result, duration_ms, started_at \
             FROM autonomy_circuit_log ORDER BY started_at DESC LIMIT 30"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok(CircuitRow {
                circuit_name: row.get(0)?,
                status: row.get(1)?,
                result: row.get(2)?,
                duration_ms: row.get(3)?,
                started_at: row.get(4)?,
            })
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(n) = r { result.push(n); } }
        Ok(result)
    }).await.map_err(|e| e.to_string())?;

    // Last 30 mcp calls
    obs.recent_mcp_calls = db.with_conn(|conn| -> Result<Vec<McpCallRow>, BrainError> {
        let mut stmt = conn.prepare(
            "SELECT command, payload, called_at FROM mcp_call_log ORDER BY called_at DESC LIMIT 30"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok(McpCallRow {
                command: row.get(0)?,
                payload: row.get(1)?,
                called_at: row.get(2)?,
            })
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(n) = r { result.push(n); } }
        Ok(result)
    }).await.map_err(|e| e.to_string())?;

    // New nodes in the last 24h
    let cutoff = (chrono::Utc::now() - chrono::Duration::hours(24)).to_rfc3339();
    let cutoff_clone = cutoff.clone();
    obs.new_nodes_24h = db.with_conn(move |conn| -> Result<u64, BrainError> {
        let count: u64 = conn.query_row(
            "SELECT COUNT(*) FROM nodes WHERE created_at > ?1",
            params![cutoff_clone],
            |row| row.get(0),
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(count)
    }).await.map_err(|e| e.to_string())?;

    let cutoff_clone2 = cutoff.clone();
    obs.new_thinking_nodes_24h = db.with_conn(move |conn| -> Result<u64, BrainError> {
        let count: u64 = conn.query_row(
            "SELECT COUNT(*) FROM nodes WHERE created_at > ?1 \
             AND node_type IN ('hypothesis', 'insight', 'decision', 'strategy', 'contradiction', 'prediction', 'synthesis')",
            params![cutoff_clone2],
            |row| row.get(0),
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(count)
    }).await.map_err(|e| e.to_string())?;

    // Top mcp commands
    let mut counts: HashMap<String, u64> = HashMap::new();
    for c in &obs.recent_mcp_calls {
        *counts.entry(c.command.clone()).or_insert(0) += 1;
    }
    let mut sorted: Vec<(String, u64)> = counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));
    sorted.truncate(5);
    obs.top_mcp_commands = sorted;

    // Circuit health
    let mut health: HashMap<String, (u64, u64)> = HashMap::new();
    for c in &obs.recent_circuits {
        let entry = health.entry(c.circuit_name.clone()).or_insert((0, 0));
        if c.status == "ok" {
            entry.0 += 1;
        } else {
            entry.1 += 1;
        }
    }
    obs.circuit_health = health.into_iter().map(|(k, (s, f))| (k, s, f)).collect();

    Ok(obs)
}

// =========================================================================
// ANALYZE
// =========================================================================

#[derive(Debug)]
struct Analysis {
    /// Circuits with > 2 failures and 0 successes — broken
    failing_circuits: Vec<String>,
    /// Topics your AI assistant is asking about repeatedly
    hot_topics_from_mcp: Vec<String>,
    /// Ratio of thinking nodes to total new nodes
    thinking_ratio: f64,
    /// Verdict: "healthy" | "stalled" | "degraded" | "growing"
    health: String,
    /// Free-text summary
    summary: String,
}

fn analyze(obs: &Observation) -> Analysis {
    let mut failing_circuits = Vec::new();
    for (name, ok, fail) in &obs.circuit_health {
        if *fail >= 2 && *ok == 0 {
            failing_circuits.push(name.clone());
        }
    }

    // Hot topics from mcp_call_log: extract distinctive query phrases
    let mut topic_counts: HashMap<String, u64> = HashMap::new();
    for c in &obs.recent_mcp_calls {
        // Use the first 60 chars of payload as a topic key (rough)
        let key: String = c.payload.chars().take(60).collect();
        let key = key.trim().to_lowercase();
        if key.len() > 10 {
            *topic_counts.entry(key).or_insert(0) += 1;
        }
    }
    let mut sorted: Vec<(String, u64)> = topic_counts.into_iter().filter(|(_, c)| *c >= 2).collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));
    let hot_topics_from_mcp: Vec<String> = sorted.into_iter().take(3).map(|(k, _)| k).collect();

    let thinking_ratio = if obs.new_nodes_24h > 0 {
        obs.new_thinking_nodes_24h as f64 / obs.new_nodes_24h as f64
    } else {
        0.0
    };

    let health = if !failing_circuits.is_empty() {
        "degraded"
    } else if obs.new_nodes_24h == 0 && obs.recent_circuits.is_empty() {
        "stalled"
    } else if thinking_ratio > 0.05 {
        "growing"
    } else {
        "healthy"
    }
    .to_string();

    let summary = format!(
        "{}: {} new nodes (24h), {} thinking, ratio {:.1}%, {} failing circuits, {} hot mcp topics",
        health,
        obs.new_nodes_24h,
        obs.new_thinking_nodes_24h,
        thinking_ratio * 100.0,
        failing_circuits.len(),
        hot_topics_from_mcp.len()
    );

    Analysis {
        failing_circuits,
        hot_topics_from_mcp,
        thinking_ratio,
        health,
        summary,
    }
}

// =========================================================================
// IMPROVE
// =========================================================================

#[derive(Debug)]
struct Improvement {
    /// New research_mission rows created
    missions_queued: u32,
    /// Was an insight node created this cycle
    insight_created: bool,
    /// Free-text summary
    summary: String,
}

async fn improve(
    db: &BrainDb,
    obs: &Observation,
    analysis: &Analysis,
) -> Result<Improvement, String> {
    let mut missions_queued = 0u32;

    // Improvement 1: queue research missions for hot mcp topics
    if !analysis.hot_topics_from_mcp.is_empty() {
        let now = chrono::Utc::now().to_rfc3339();
        for topic in &analysis.hot_topics_from_mcp {
            // Skip if already a research mission for this topic
            let topic_clone = topic.clone();
            let existing: Vec<String> = db.with_conn(|conn| -> Result<Vec<String>, BrainError> {
                let mut stmt = conn.prepare(
                    "SELECT topic FROM research_missions \
                     WHERE status IN ('pending', 'in_progress') LIMIT 100"
                ).map_err(|e| BrainError::Database(e.to_string()))?;
                let rows = stmt.query_map([], |row| {
                    Ok(row.get::<_, String>(0)?)
                }).map_err(|e| BrainError::Database(e.to_string()))?;
                let mut result = Vec::new();
                for r in rows { if let Ok(t) = r { result.push(t); } }
                Ok(result)
            }).await.map_err(|e| e.to_string())?;

            if existing.iter().any(|e| crate::circuits::similar_text(e, &topic_clone) > 0.6) {
                continue;
            }

            let topic_for_insert = topic.clone();
            let now_for_insert = now.clone();
            let _ = db.with_conn(move |conn| -> Result<(), BrainError> {
                let id = format!("research_missions:{}", uuid::Uuid::now_v7());
                conn.execute(
                    "INSERT INTO research_missions (id, topic, status, source, priority, created_at) \
                     VALUES (?1, ?2, 'pending', 'master_loop', 'high', ?3)",
                    params![id, topic_for_insert, now_for_insert],
                ).map_err(|e| BrainError::Database(e.to_string()))?;
                Ok(())
            }).await;
            missions_queued += 1;
            if missions_queued >= 3 { break; }
        }
    }

    // Improvement 2: if any circuits are failing, log a meta insight about it
    if !analysis.failing_circuits.is_empty() {
        log::warn!(
            "Master loop noticed failing circuits: {:?}",
            analysis.failing_circuits
        );
    }

    // Improvement 3 (optional): ask the LLM to generate one strategic insight
    // about the cycle, but only if there's enough material.
    let mut insight_created = false;
    if obs.recent_circuits.len() >= 5 || obs.new_nodes_24h >= 10 {
        if let Ok(insight) = generate_meta_insight(db, obs, analysis).await {
            if let Err(e) = create_insight_node(db, &insight).await {
                log::warn!("Failed to create master loop insight node: {}", e);
            } else {
                insight_created = true;
            }
        }
    }

    let summary = format!(
        "queued {} missions, insight={}",
        missions_queued, insight_created
    );

    Ok(Improvement {
        missions_queued,
        insight_created,
        summary,
    })
}

async fn generate_meta_insight(
    db: &BrainDb,
    obs: &Observation,
    analysis: &Analysis,
) -> Result<String, String> {
    // FAST tier: master loop summarises observations, doesn't deep-reason
    let llm = get_llm_client_fast(db);

    let mut circuit_summary = String::new();
    for c in obs.recent_circuits.iter().take(15) {
        circuit_summary.push_str(&format!(
            "- [{}] {} ({}ms): {}\n",
            c.status,
            c.circuit_name,
            c.duration_ms,
            crate::circuits::short(&c.result, 100)
        ));
    }

    let prompt = format!(
        "You are the meta-monitor for a self-evolving knowledge brain. Given this snapshot of \
         recent activity, write ONE specific, actionable insight in 2-3 sentences. Focus on what \
         the brain should pay attention to next. No preamble.\n\n\
         Health: {}\n\
         Recent circuit runs:\n{}\n\
         New nodes (24h): {}, thinking nodes: {}, ratio: {:.1}%\n\
         Top user/MCP topics: {:?}\n\
         Failing circuits: {:?}",
        analysis.health,
        circuit_summary,
        obs.new_nodes_24h,
        obs.new_thinking_nodes_24h,
        analysis.thinking_ratio * 100.0,
        analysis.hot_topics_from_mcp,
        analysis.failing_circuits,
    );

    llm.generate(&prompt, 200).await.map_err(|e| e.to_string())
}

async fn create_insight_node(db: &BrainDb, content: &str) -> Result<(), String> {
    let title = format!(
        "Master loop insight: {}",
        chrono::Utc::now().format("%Y-%m-%d %H:%M")
    );
    let input = CreateNodeInput {
        title,
        content: content.to_string(),
        domain: "synthesis".into(),
        topic: "master-loop".into(),
        tags: vec!["master_loop".into(), "meta".into(), "insight".into()],
        node_type: NODE_TYPE_INSIGHT.to_string(),
        source_type: "synthesis".into(),
        source_url: None,
    };
    let created = db.create_node(input).await.map_err(|e| e.to_string())?;

    // Mark as synthesized_by_brain
    let id = created.id.clone();
    let _ = db.with_conn(move |conn| -> Result<(), BrainError> {
        conn.execute(
            "UPDATE nodes SET synthesized_by_brain = 1, cognitive_type = ?1, confidence = 0.8 WHERE id = ?2",
            params![NODE_TYPE_INSIGHT, id],
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(())
    }).await;
    Ok(())
}

// =========================================================================
// ACT
// =========================================================================

async fn act(
    db: &BrainDb,
    obs: &Observation,
    analysis: &Analysis,
    improvement: &Improvement,
    started_at: &str,
) -> Result<(), String> {
    let id = format!("master_loop_log:{}", uuid::Uuid::now_v7());
    let created_at = started_at.to_string();

    // Build a combined phase string: "observe->analyze->improve->act"
    let phase = format!("{} | missions={} insight={}", analysis.health, improvement.missions_queued, improvement.insight_created);

    // Build a combined result string with all the details
    let result = format!(
        "{} | nodes_24h={} thinking={} ratio={:.1}% failing={} topics={}",
        analysis.summary,
        obs.new_nodes_24h,
        obs.new_thinking_nodes_24h,
        analysis.thinking_ratio * 100.0,
        serde_json::to_string(&analysis.failing_circuits).unwrap_or_default(),
        serde_json::to_string(&analysis.hot_topics_from_mcp).unwrap_or_default(),
    );

    let _ = db.with_conn(move |conn| -> Result<(), BrainError> {
        conn.execute(
            "INSERT INTO master_loop_log (id, phase, result, created_at) \
             VALUES (?1, ?2, ?3, ?4)",
            params![id, phase, result, created_at],
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(())
    }).await;

    Ok(())
}
