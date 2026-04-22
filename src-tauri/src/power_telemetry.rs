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
// Circuit profiles (Phase 3)
// =========================================================================

/// Latency class of a circuit. Routes inference to the backend that fits
/// its tolerance for wait time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitProfile {
    /// User is actively waiting — GPU-first, never downshift.
    Interactive,
    /// User may glance at the result but isn't blocked — GPU by default,
    /// CPU acceptable under load.
    NearRealTime,
    /// Pure background work — CPU is the default to save power.
    Batch,
}

impl CircuitProfile {
    #[allow(dead_code)]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Interactive => "interactive",
            Self::NearRealTime => "near_real_time",
            Self::Batch => "batch",
        }
    }
}

/// Look up the profile of a circuit by name.
///
/// Explicitly-scoped batch circuits (those firing through `run_circuit()`)
/// are matched by name and routed to CPU. Any name not in the table —
/// including calls made outside a scope, which land as "unknown" — gets
/// Interactive so ad-hoc / user-initiated calls don't accidentally slow
/// down on CPU. To keep a circuit on GPU, add it to the Interactive or
/// NearRealTime arms below.
pub fn circuit_profile(name: &str) -> CircuitProfile {
    match name {
        // Interactive — user is blocked waiting
        "chat_response"
        | "brain_recall"
        | "sidekick"
        | "sidekick_suggestions"
        | "unknown"
            => CircuitProfile::Interactive,

        // Near real-time — user may check soon, but no active wait
        "session_summarizer"
        | "anticipatory_preloader"
        | "morning_briefing"
        | "context_quality_optimizer"
            => CircuitProfile::NearRealTime,

        // Every *scoped* circuit (i.e. called via run_circuit) that we
        // don't explicitly list above is a background rotation circuit
        // and should go to CPU to save power.
        //
        // Interactive circuits that fire outside run_circuit() will match
        // "unknown" above and stay on GPU.
        "meta_reflection"
        | "user_pattern_mining"
        | "cross_domain_fusion"
        | "quality_recalc"
        | "self_synthesis"
        | "curiosity_gap_fill"
        | "iq_boost"
        | "compression_cycle"
        | "contradiction_detector"
        | "decision_memory_extractor"
        | "knowledge_synthesizer"
        | "self_assessment"
        | "prediction_validator"
        | "hypothesis_tester"
        | "code_pattern_extractor"
        | "synapse_prune"
        | "fingerprint_synthesis"
        | "internal_dialogue"
        | "swarm_orchestrator"
        | "temporal_analysis"
        | "causal_model_builder"
        | "scenario_simulator"
        | "knowledge_compiler"
        | "circuit_optimizer"
        | "capability_tracker"
        | "self_reflection"
        | "attention_update"
        | "curiosity_v2"
        | "federation_sync"
        | "cluster_health_check"
        | "economic_audit"
        | "deep_synthesis"
            => CircuitProfile::Batch,

        // Unknown scoped names default to NearRealTime — safer than Batch
        // when we don't know what they're doing.
        _ => CircuitProfile::NearRealTime,
    }
}

/// Profile for the circuit on whose behalf this task is running.
pub fn current_profile() -> CircuitProfile {
    circuit_profile(&current_circuit())
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
#[allow(dead_code)]
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

/// Same as `record_inference_global` but takes the circuit name explicitly
/// instead of reading the task-local. Callers that dispatch the recorder
/// via `tokio::spawn` must use this variant — task-locals do not propagate
/// across spawn boundaries.
pub async fn record_inference_with_circuit(
    circuit: &str,
    backend: &str,
    model: &str,
    tokens_in: u32,
    tokens_out: u32,
    duration_ms: u64,
) {
    let Some(db) = TELEMETRY_DB.get().cloned() else { return };
    let energy_wh = estimate_energy_wh(backend, duration_ms);
    let id = format!("inf:{}", uuid::Uuid::now_v7());
    let created_at = chrono::Utc::now().to_rfc3339();
    let circuit = circuit.to_string();
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
    /// Average watts while actively generating (duration-weighted).
    pub avg_watts: f64,
    /// Projected annual energy if the measured window is representative.
    /// Calculated as `total_energy_wh * (8760 / window_hours) / 1000`.
    /// Phase 6 — gives users a single "$/year" figure to optimize against.
    pub annualized_kwh: f64,
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

        let annualized_kwh =
            total_energy_wh * (8760.0 / window_hours.max(1) as f64) / 1000.0;

        Ok(PowerSummary {
            window_hours,
            total_calls: total_calls as u64,
            total_energy_wh,
            avg_watts,
            annualized_kwh,
            by_circuit,
            by_backend,
        })
    })
    .await
}

// =========================================================================
// Live policy snapshot (Phase 6)
// =========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerStatus {
    /// Currently active PowerMode (normal / eco / ...).
    pub mode: &'static str,
    /// Whether the adaptive policy currently wants to demote to CPU.
    pub prefer_cpu: bool,
    /// Whether a CPU Ollama daemon is configured at all.
    pub cpu_daemon_configured: bool,
    /// AC line detection: `Some(true)` = on battery, `Some(false)` = mains,
    /// `None` = unknown / unsupported OS.
    pub on_battery: Option<bool>,
    /// Active wattage coefficients used by `estimate_energy_wh`.
    pub backend_watts: std::collections::BTreeMap<String, f64>,
}

pub fn power_status(db: &BrainDb) -> PowerStatus {
    let mut backend_watts = std::collections::BTreeMap::new();
    for b in ["ollama-vulkan", "ollama-gpu", "ollama-rocm", "ollama-cpu", "anthropic-api", "peer-rpc"] {
        backend_watts.insert(b.to_string(), backend_active_watts(b));
    }
    PowerStatus {
        mode: crate::power_policy::current_mode().as_str(),
        prefer_cpu: crate::power_policy::prefer_cpu(),
        cpu_daemon_configured: db.config.ollama_cpu_url.is_some(),
        on_battery: crate::power_policy::on_battery(),
        backend_watts,
    }
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
