//! Self-Improvement commands — Phase Omega Part IV
//!
//! Tauri IPC handlers for knowledge rules, circuit performance, and
//! capability tracking.

use crate::capability_frontier::Capability;
use crate::circuit_performance::CircuitPerformance;
use crate::db::BrainDb;
use crate::error::BrainError;
use crate::knowledge_compiler::{self, KnowledgeRule};
use std::sync::Arc;
use tauri::State;

// =========================================================================
// Knowledge Rules
// =========================================================================

#[tauri::command]
pub async fn get_knowledge_rules(
    db: State<'_, Arc<BrainDb>>,
) -> Result<Vec<KnowledgeRule>, BrainError> {
    db.with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT id, source_node_ids, rule_type, condition, action,
                        confidence, times_applied, times_correct, accuracy,
                        compiled_at, invalidated
                 FROM knowledge_rules
                 WHERE invalidated = 0
                 ORDER BY confidence DESC
                 LIMIT 200",
            )
            .map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(KnowledgeRule {
                    id: row.get(0)?,
                    source_node_ids: serde_json::from_str(&row.get::<_, String>(1)?)
                        .unwrap_or_default(),
                    rule_type: row.get(2)?,
                    condition: row.get(3)?,
                    action: row.get(4)?,
                    confidence: row.get(5)?,
                    times_applied: row.get::<_, u32>(6)?,
                    times_correct: row.get::<_, u32>(7)?,
                    accuracy: row.get(8)?,
                    compiled_at: row.get(9)?,
                    invalidated: row.get::<_, i32>(10)? != 0,
                })
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
    .await
}

// =========================================================================
// Circuit Performance
// =========================================================================

#[tauri::command]
pub async fn get_circuit_performance(
    db: State<'_, Arc<BrainDb>>,
) -> Result<Vec<CircuitPerformance>, BrainError> {
    db.with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT circuit_name, total_runs, success_runs, avg_duration_ms,
                        nodes_created, edges_created, iq_delta, efficiency
                 FROM circuit_performance
                 ORDER BY efficiency DESC",
            )
            .map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(CircuitPerformance {
                    circuit_name: row.get(0)?,
                    total_runs: row.get(1)?,
                    success_runs: row.get(2)?,
                    avg_duration_ms: row.get::<_, i64>(3)? as u64,
                    nodes_created: row.get(4)?,
                    edges_created: row.get(5)?,
                    iq_delta: row.get(6)?,
                    efficiency: row.get(7)?,
                })
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
    .await
}

// =========================================================================
// Capabilities
// =========================================================================

#[tauri::command]
pub async fn get_capabilities(
    db: State<'_, Arc<BrainDb>>,
) -> Result<Vec<Capability>, BrainError> {
    db.with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT id, name, proficiency, evidence_count, last_tested,
                        status, improvement_plan
                 FROM capabilities
                 ORDER BY proficiency DESC",
            )
            .map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(Capability {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    proficiency: row.get(2)?,
                    evidence_count: row.get(3)?,
                    last_tested: row.get::<_, String>(4).unwrap_or_default(),
                    status: row.get(5)?,
                    improvement_plan: row.get(6)?,
                })
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
    .await
}

// =========================================================================
// Compile Rules Now (manual trigger)
// =========================================================================

#[tauri::command]
pub async fn compile_rules_now(
    db: State<'_, Arc<BrainDb>>,
) -> Result<String, BrainError> {
    let db_arc: Arc<BrainDb> = (*db).clone();
    knowledge_compiler::compile_rules(&db_arc).await
}
