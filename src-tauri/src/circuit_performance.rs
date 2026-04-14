//! Circuit Performance Tracker — Phase Omega Part IV
//!
//! Aggregates runtime data from `autonomy_circuit_log` to compute per-circuit
//! efficiency metrics, ranks circuits, and uses the LLM to suggest parameter
//! tweaks for underperformers.

use crate::db::BrainDb;
use crate::error::BrainError;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// =========================================================================
// Types
// =========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitPerformance {
    pub circuit_name: String,
    pub total_runs: u32,
    pub success_runs: u32,
    pub avg_duration_ms: u64,
    pub nodes_created: u32,
    pub edges_created: u32,
    pub iq_delta: f32,
    pub efficiency: f32,
}

// =========================================================================
// compute_circuit_performance — aggregate from autonomy_circuit_log
// =========================================================================

pub async fn compute_circuit_performance(
    db: &Arc<BrainDb>,
) -> Result<Vec<CircuitPerformance>, BrainError> {
    let raw: Vec<(String, u32, u32, u64)> = db
        .with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT circuit_name,
                            COUNT(*) as total,
                            SUM(CASE WHEN status = 'ok' THEN 1 ELSE 0 END) as successes,
                            AVG(duration_ms) as avg_dur
                     FROM autonomy_circuit_log
                     GROUP BY circuit_name
                     ORDER BY total DESC",
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, u32>(1)?,
                        row.get::<_, u32>(2)?,
                        row.get::<_, f64>(3).unwrap_or(0.0) as u64,
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

    // Count nodes and edges created by each circuit by parsing result strings
    // from circuit logs. The typical result pattern is "Created X nodes" or
    // "Created X edges". We do a best-effort parse.
    let result_texts: Vec<(String, String)> = db
        .with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT circuit_name, result
                     FROM autonomy_circuit_log
                     WHERE status = 'ok'
                     ORDER BY started_at DESC
                     LIMIT 500",
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt
                .query_map([], |row| {
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

    // Aggregate counts per circuit from result text
    let mut nodes_by_circuit: std::collections::HashMap<String, u32> =
        std::collections::HashMap::new();
    let mut edges_by_circuit: std::collections::HashMap<String, u32> =
        std::collections::HashMap::new();

    for (name, result) in &result_texts {
        let lower = result.to_lowercase();
        // Parse patterns like "Created 5 cross-domain edges" or "3 quality + 2 decay"
        for word_pair in lower.split_whitespace().collect::<Vec<_>>().windows(2) {
            if let Ok(n) = word_pair[0].parse::<u32>() {
                let next = word_pair[1];
                if next.contains("node") || next.contains("insight") || next.contains("synthe") {
                    *nodes_by_circuit.entry(name.clone()).or_insert(0) += n;
                } else if next.contains("edge") || next.contains("link") || next.contains("bridge")
                    || next.contains("synapse") || next.contains("fusion")
                {
                    *edges_by_circuit.entry(name.clone()).or_insert(0) += n;
                }
            }
        }
    }

    let now = chrono::Utc::now().to_rfc3339();
    let mut performances = Vec::new();

    for (name, total, successes, avg_dur) in &raw {
        let success_rate = if *total > 0 {
            *successes as f32 / *total as f32
        } else {
            0.0
        };
        let nodes = *nodes_by_circuit.get(name).unwrap_or(&0);
        let edges = *edges_by_circuit.get(name).unwrap_or(&0);

        // Efficiency = success_rate * (nodes + edges produced) / max(avg_duration_seconds, 1)
        let dur_secs = (*avg_dur as f32 / 1000.0).max(1.0);
        let output = (nodes + edges) as f32;
        let efficiency = success_rate * (1.0 + output) / dur_secs;

        let perf = CircuitPerformance {
            circuit_name: name.clone(),
            total_runs: *total,
            success_runs: *successes,
            avg_duration_ms: *avg_dur,
            nodes_created: nodes,
            edges_created: edges,
            iq_delta: 0.0, // Would require pre/post IQ tracking
            efficiency,
        };

        // Persist to circuit_performance table
        let cn = name.clone();
        let tr = *total;
        let sr = *successes;
        let ad = *avg_dur;
        let nc = nodes;
        let ec = edges;
        let eff = efficiency;
        let ts = now.clone();
        let _ = db
            .with_conn(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO circuit_performance
                     (circuit_name, total_runs, success_runs, avg_duration_ms,
                      nodes_created, edges_created, iq_delta, efficiency, last_computed)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0.0, ?7, ?8)",
                    params![cn, tr, sr, ad, nc, ec, eff, ts],
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
                Ok(())
            })
            .await;

        performances.push(perf);
    }

    Ok(performances)
}

// =========================================================================
// rank_circuits — rank all circuits by efficiency
// =========================================================================

pub async fn rank_circuits(db: &Arc<BrainDb>) -> Result<Vec<CircuitPerformance>, BrainError> {
    let mut perfs = compute_circuit_performance(db).await?;
    perfs.sort_by(|a, b| {
        b.efficiency
            .partial_cmp(&a.efficiency)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(perfs)
}

// =========================================================================
// suggest_improvements — LLM-driven analysis of worst performers
// =========================================================================

pub async fn suggest_improvements(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    let ranked = rank_circuits(db).await?;

    if ranked.is_empty() {
        return Ok("No circuit performance data yet".to_string());
    }

    // Take the bottom 5 (worst performing) circuits
    let worst: Vec<&CircuitPerformance> = ranked.iter().rev().take(5).collect();

    let mut context = String::from("CIRCUIT PERFORMANCE DATA (worst performing):\n\n");
    for p in &worst {
        context.push_str(&format!(
            "Circuit: {}\n  Total runs: {}, Success: {} ({:.0}%)\n  \
             Avg duration: {}ms\n  Nodes created: {}, Edges created: {}\n  \
             Efficiency score: {:.4}\n\n",
            p.circuit_name,
            p.total_runs,
            p.success_runs,
            if p.total_runs > 0 {
                p.success_runs as f32 / p.total_runs as f32 * 100.0
            } else {
                0.0
            },
            p.avg_duration_ms,
            p.nodes_created,
            p.edges_created,
            p.efficiency,
        ));
    }

    context.push_str("TOP PERFORMING circuits for comparison:\n");
    for p in ranked.iter().take(3) {
        context.push_str(&format!(
            "  {} — efficiency={:.4}, success={:.0}%, avg={}ms\n",
            p.circuit_name,
            p.efficiency,
            if p.total_runs > 0 {
                p.success_runs as f32 / p.total_runs as f32 * 100.0
            } else {
                0.0
            },
            p.avg_duration_ms,
        ));
    }

    let prompt = format!(
        "You are optimizing the brain's self-improvement circuits. \
         Analyze the worst-performing circuits and suggest concrete improvements.\n\n\
         For each circuit, suggest:\n\
         1. Why it might be underperforming\n\
         2. Specific parameter changes (e.g., batch size, threshold, frequency)\n\
         3. Whether it should be deprecated or merged with another circuit\n\n\
         Be concise and actionable. No filler.\n\n{}",
        context
    );

    let llm = crate::commands::ai::get_llm_client_fast(db);
    let suggestions = llm.generate(&prompt, 800).await?;

    Ok(suggestions)
}
