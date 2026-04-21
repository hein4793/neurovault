//! Phase 1 — Power & cost telemetry.
//!
//! Every LLM inference call records one row into `inference_log`. The circuit
//! name is carried via a tokio task-local set up by the circuit dispatcher,
//! so individual call sites don't have to thread it through their signatures.
//!
//! Energy estimates use per-backend coefficients (based on hardware
//! characterizations — i7-12700F ~80W under CPU inference, RX 6900 XT
//! ~300W under GPU inference). These are first-pass values meant for
//! *relative* comparisons between circuits/backends; Phase 6 will calibrate
//! them against actual wall-power measurements.

use crate::db::BrainDb;
use crate::error::BrainError;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// =========================================================================
// Task-local circuit name
// =========================================================================

tokio::task_local! {
    /// Name of the circuit that owns the current task. Set by the circuit
    /// dispatcher; read by `current_circuit()` inside inference call sites.
    pub static CURRENT_CIRCUIT: String;
}

/// Return the name of the circuit on whose behalf the current task is
/// running. Falls back to `"unknown"` when called outside a circuit scope
/// (e.g. user-initiated HTTP handlers).
pub fn current_circuit() -> String {
    CURRENT_CIRCUIT
        .try_with(|s| s.clone())
        .unwrap_or_else(|_| "unknown".to_string())
}

// =========================================================================
// Energy coefficients
// =========================================================================

/// Approximate additional power draw (above idle) of each backend while
/// actively generating, in watts. Derived from hardware specs; see
/// module docs.
const fn backend_active_watts(backend: &str) -> f64 {
    match backend.as_bytes() {
        b"ollama-vulkan" | b"ollama-gpu"      => 300.0,
        b"ollama-rocm"                        => 280.0,
        b"ollama-cpu"                         => 80.0,
        b"anthropic-api" | b"peer-rpc"        => 0.0, // network only; offloaded
        _                                     => 100.0,
    }
}

/// Convert a call duration into an energy estimate in watt-hours.
pub fn estimate_energy_wh(backend: &str, duration_ms: u64) -> f64 {
    let seconds = duration_ms as f64 / 1000.0;
    let watts = backend_active_watts(backend);
    watts * seconds / 3600.0
}

// =========================================================================
// Global telemetry DB accessor
// =========================================================================

// Schema for `inference_log` lives in `db::BrainDb::init_schema` alongside
// the rest of the core tables so it's created once at DB init.

use std::sync::OnceLock;
static TELEMETRY_DB: OnceLock<Arc<BrainDb>> = OnceLock::new();

/// Register the BrainDb that power_telemetry should write to. Called once
/// from `lib.rs` right after `BrainDb::init()` completes. Subsequent calls
/// are no-ops.
pub fn init_telemetry_db(db: Arc<BrainDb>) {
    let _ = TELEMETRY_DB.set(db);
}

/// Record an inference via the globally-registered DB. No-op when telemetry
/// hasn't been initialized (tests, bootstrap).
pub async fn record_inference_global(
    backend: &str,
    model: &str,
    tokens_in: u32,
    tokens_out: u32,
    duration_ms: u64,
) {
    let Some(db) = TELEMETRY_DB.get().cloned() else { return };
    record_inference(&db, backend, model, tokens_in, tokens_out, duration_ms).await;
}

// =========================================================================
// Recording
// =========================================================================

/// Record one completed inference. Circuit name is read from task-local.
/// Failures are logged but never propagated — telemetry must not break inference.
pub async fn record_inference(
    db: &Arc<BrainDb>,
    backend: &str,
    model: &str,
    tokens_in: u32,
    tokens_out: u32,
    duration_ms: u64,
) {
    let circuit = current_circuit();
    let energy_wh = estimate_energy_wh(backend, duration_ms);
    let id = format!("inf:{}", uuid::Uuid::now_v7());
    let created_at = chrono::Utc::now().to_rfc3339();
    let backend = backend.to_string();
    let model = model.to_string();

    let result = db
        .with_conn(move |conn| {
            conn.execute(
                "INSERT INTO inference_log
                     (id, circuit, backend, model, tokens_in, tokens_out,
                      duration_ms, energy_wh, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    id,
                    circuit,
                    backend,
                    model,
                    tokens_in as i64,
                    tokens_out as i64,
                    duration_ms as i64,
                    energy_wh,
                    created_at,
                ],
            )
            .map_err(|e| BrainError::Database(e.to_string()))?;
            Ok(())
        })
        .await;

    if let Err(e) = result {
        log::warn!("power_telemetry: failed to record inference: {}", e);
    }
}

// =========================================================================
// Rollups — read side
// =========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitPower {
    pub circuit: String,
    pub calls: u64,
    pub energy_wh: f64,
    pub total_duration_ms: u64,
    pub avg_duration_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendPower {
    pub backend: String,
    pub calls: u64,
    pub energy_wh: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerSummary {
    pub window_hours: i64,
    pub total_calls: u64,
    pub total_energy_wh: f64,
    pub avg_watts: f64,
    pub by_circuit: Vec<CircuitPower>,
    pub by_backend: Vec<BackendPower>,
}

/// Full rollup for the last `window_hours`. Used by `/metrics/power`.
pub async fn rollup_power(db: &Arc<BrainDb>, window_hours: i64) -> Result<PowerSummary, BrainError> {
    let cutoff = (chrono::Utc::now() - chrono::Duration::hours(window_hours)).to_rfc3339();

    db.with_conn(move |conn| {
        // Total
        let (total_calls, total_energy_wh, total_duration_ms): (i64, f64, i64) = conn
            .query_row(
                "SELECT COUNT(*), COALESCE(SUM(energy_wh), 0.0), COALESCE(SUM(duration_ms), 0)
                   FROM inference_log
                  WHERE created_at >= ?1",
                params![cutoff],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .map_err(|e| BrainError::Database(e.to_string()))?;

        // Per-circuit
        let mut by_circuit = Vec::new();
        {
            let mut stmt = conn
                .prepare(
                    "SELECT circuit,
                            COUNT(*)                 AS calls,
                            COALESCE(SUM(energy_wh), 0.0) AS energy_wh,
                            COALESCE(SUM(duration_ms), 0) AS total_duration,
                            COALESCE(AVG(duration_ms), 0.0) AS avg_duration
                       FROM inference_log
                      WHERE created_at >= ?1
                      GROUP BY circuit
                      ORDER BY energy_wh DESC",
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt
                .query_map(params![cutoff], |row| {
                    Ok(CircuitPower {
                        circuit: row.get(0)?,
                        calls: row.get::<_, i64>(1)? as u64,
                        energy_wh: row.get(2)?,
                        total_duration_ms: row.get::<_, i64>(3)? as u64,
                        avg_duration_ms: row.get(4)?,
                    })
                })
                .map_err(|e| BrainError::Database(e.to_string()))?;
            for r in rows {
                if let Ok(cp) = r {
                    by_circuit.push(cp);
                }
            }
        }

        // Per-backend
        let mut by_backend = Vec::new();
        {
            let mut stmt = conn
                .prepare(
                    "SELECT backend, COUNT(*), COALESCE(SUM(energy_wh), 0.0)
                       FROM inference_log
                      WHERE created_at >= ?1
                      GROUP BY backend
                      ORDER BY 3 DESC",
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt
                .query_map(params![cutoff], |row| {
                    Ok(BackendPower {
                        backend: row.get(0)?,
                        calls: row.get::<_, i64>(1)? as u64,
                        energy_wh: row.get(2)?,
                    })
                })
                .map_err(|e| BrainError::Database(e.to_string()))?;
            for r in rows {
                if let Ok(bp) = r {
                    by_backend.push(bp);
                }
            }
        }

        // Average watts: energy is per-call while the backend was active;
        // dividing by total *active* seconds gives the weighted-average
        // draw while generating. (Wall-clock averaging — including idle —
        // would need a separate sampler.)
        let active_seconds = (total_duration_ms as f64 / 1000.0).max(1.0);
        let avg_watts = (total_energy_wh * 3600.0) / active_seconds;

        Ok(PowerSummary {
            window_hours,
            total_calls: total_calls as u64,
            total_energy_wh,
            avg_watts,
            by_circuit,
            by_backend,
        })
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn energy_cpu_is_lower_than_gpu() {
        let gpu = estimate_energy_wh("ollama-vulkan", 10_000);
        let cpu = estimate_energy_wh("ollama-cpu", 10_000);
        assert!(gpu > cpu);
        assert!(cpu > 0.0);
    }

    #[test]
    fn energy_api_is_zero_local() {
        assert_eq!(estimate_energy_wh("anthropic-api", 10_000), 0.0);
    }

    #[test]
    fn energy_scales_with_duration() {
        let short = estimate_energy_wh("ollama-vulkan", 1_000);
        let long = estimate_energy_wh("ollama-vulkan", 10_000);
        assert!((long / short - 10.0).abs() < 0.001);
    }

    #[tokio::test]
    async fn current_circuit_outside_scope_returns_unknown() {
        assert_eq!(current_circuit(), "unknown");
    }

    #[tokio::test]
    async fn current_circuit_inside_scope_returns_name() {
        let got = CURRENT_CIRCUIT
            .scope("test_circuit".to_string(), async { current_circuit() })
            .await;
        assert_eq!(got, "test_circuit");
    }
}
