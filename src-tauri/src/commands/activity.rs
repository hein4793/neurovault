//! Brain Activity — Phase 4.7 of the master plan.
//!
//! Returns a single snapshot of "what's the brain doing right now" for
//! the new BrainActivityPanel. Aggregates data from four log tables
//! (`autonomy_circuit_log`, `master_loop_log`, `memory_tier_log`,
//! `fine_tune_run`) into one Tauri call so the panel can poll cheaply.

use crate::db::BrainDb;
use crate::error::BrainError;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitLogEntry {
    pub circuit_name: String,
    pub started_at: String,
    pub duration_ms: u64,
    pub status: String,
    pub result: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MasterLoopEntry {
    pub started_at: String,
    pub phase: String,
    pub result: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryTierEntry {
    pub ran_at: String,
    pub stats: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FineTuneEntry {
    pub id: String,
    pub status: String,
    pub dataset_size: u64,
    pub started_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitHealth {
    pub circuit_name: String,
    pub success_count: u64,
    pub fail_count: u64,
    pub avg_duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainActivitySnapshot {
    pub recent_circuits: Vec<CircuitLogEntry>,
    pub recent_master_loops: Vec<MasterLoopEntry>,
    pub recent_memory_tier_passes: Vec<MemoryTierEntry>,
    pub pending_fine_tunes: Vec<FineTuneEntry>,
    pub circuit_health: Vec<CircuitHealth>,
    /// Wall-clock timestamp of when this snapshot was generated.
    pub generated_at: String,
}

/// Phase 1.2C — return the most recent suggestions snapshot from the
/// sidekick suggestion engine. Reads from disk so it's cheap.
#[tauri::command]
pub async fn get_active_suggestions(
    db: State<'_, Arc<BrainDb>>,
) -> Result<crate::sidekick_suggestions::ActiveSuggestions, BrainError> {
    crate::sidekick_suggestions::load_active_suggestions(&db)
        .await
        .map_err(BrainError::Internal)
}

/// One Tauri call returns everything the activity panel needs.
#[tauri::command]
pub async fn get_brain_activity(
    db: State<'_, Arc<BrainDb>>,
) -> Result<BrainActivitySnapshot, BrainError> {
    // Last 30 circuit runs
    let recent_circuits: Vec<CircuitLogEntry> = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT circuit_name, started_at, duration_ms, status, result \
             FROM autonomy_circuit_log ORDER BY started_at DESC LIMIT 30"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok(CircuitLogEntry {
                circuit_name: row.get(0)?,
                started_at: row.get(1)?,
                duration_ms: row.get::<_, i64>(2)? as u64,
                status: row.get(3)?,
                result: row.get(4)?,
            })
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(e) = r { result.push(e); } }
        Ok(result)
    }).await?;

    // Last 10 master loop cycles
    let recent_master_loops: Vec<MasterLoopEntry> = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT created_at, phase, result \
             FROM master_loop_log ORDER BY created_at DESC LIMIT 10"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok(MasterLoopEntry {
                started_at: row.get(0)?,
                phase: row.get(1)?,
                result: row.get(2)?,
            })
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(e) = r { result.push(e); } }
        Ok(result)
    }).await?;

    // Last 10 tier passes
    let recent_memory_tier_passes: Vec<MemoryTierEntry> = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT created_at, stats FROM memory_tier_log ORDER BY created_at DESC LIMIT 10"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok(MemoryTierEntry {
                ran_at: row.get(0)?,
                stats: row.get(1)?,
            })
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(e) = r { result.push(e); } }
        Ok(result)
    }).await?;

    // Pending or recent fine-tune runs
    let pending_fine_tunes: Vec<FineTuneEntry> = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, status, dataset_size, started_at, completed_at \
             FROM fine_tune_run ORDER BY started_at DESC LIMIT 10"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok(FineTuneEntry {
                id: row.get(0)?,
                status: row.get(1)?,
                dataset_size: row.get::<_, i64>(2)? as u64,
                started_at: row.get(3)?,
                completed_at: row.get(4)?,
            })
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(e) = r { result.push(e); } }
        Ok(result)
    }).await?;

    // Circuit health rollup — group the recent circuit runs
    let mut by_name: std::collections::HashMap<String, (u64, u64, u64)> = std::collections::HashMap::new();
    for c in &recent_circuits {
        let entry = by_name.entry(c.circuit_name.clone()).or_insert((0, 0, 0));
        if c.status == "ok" {
            entry.0 += 1;
        } else {
            entry.1 += 1;
        }
        entry.2 += c.duration_ms;
    }
    let mut circuit_health: Vec<CircuitHealth> = by_name
        .into_iter()
        .map(|(name, (s, f, total_ms))| {
            let avg = if (s + f) > 0 { total_ms / (s + f) } else { 0 };
            CircuitHealth {
                circuit_name: name,
                success_count: s,
                fail_count: f,
                avg_duration_ms: avg,
            }
        })
        .collect();
    circuit_health.sort_by(|a, b| (b.success_count + b.fail_count).cmp(&(a.success_count + a.fail_count)));

    Ok(BrainActivitySnapshot {
        recent_circuits,
        recent_master_loops,
        recent_memory_tier_passes,
        pending_fine_tunes,
        circuit_health,
        generated_at: chrono::Utc::now().to_rfc3339(),
    })
}
