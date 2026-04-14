//! Phase Omega Part VII — System Health Monitor
//!
//! Tracks the brain's own compute resources: CPU, memory, disk, DB
//! size, HNSW index size, embedding queue depth, uptime, and Ollama
//! availability. Used by the UI dashboard and the throttling logic to
//! prevent overload.

use crate::db::BrainDb;
use crate::error::BrainError;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// =========================================================================
// Uptime tracking — stored as a process-lifetime static
// =========================================================================

static START_TIME: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

/// Call once at startup to record the process start time.
pub fn mark_start() {
    let _ = START_TIME.get_or_init(std::time::Instant::now);
}

fn uptime_seconds() -> u64 {
    START_TIME
        .get()
        .map(|t| t.elapsed().as_secs())
        .unwrap_or(0)
}

// =========================================================================
// Models
// =========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemHealth {
    pub cpu_usage_percent: f32,
    pub memory_used_mb: u64,
    pub memory_total_mb: u64,
    pub disk_used_mb: u64,
    pub db_size_mb: u64,
    pub hnsw_size_mb: u64,
    pub embedding_queue_size: u64,
    pub uptime_seconds: u64,
    pub ollama_available: bool,
    pub ollama_models: Vec<String>,
}

// =========================================================================
// Core function
// =========================================================================

/// Gather system health metrics: CPU, memory, disk, DB size, HNSW size,
/// embedding queue depth, Ollama status.
pub async fn get_system_health(db: &Arc<BrainDb>) -> Result<SystemHealth, BrainError> {
    // Ensure the start time is recorded (idempotent)
    mark_start();

    // DB file size
    let db_path = db.config.sqlite_path();
    let db_size_bytes = std::fs::metadata(&db_path)
        .map(|m| m.len())
        .unwrap_or(0);
    let db_size_mb = db_size_bytes / (1024 * 1024);

    // HNSW index file size
    let hnsw_path = db.config.hnsw_index_path();
    let hnsw_size_bytes = std::fs::metadata(&hnsw_path)
        .map(|m| m.len())
        .unwrap_or(0);
    let hnsw_size_mb = hnsw_size_bytes / (1024 * 1024);

    // Embedding queue size — count nodes without embeddings
    let eq_size: u64 = db
        .with_conn(|conn| {
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM nodes WHERE id NOT IN (SELECT node_id FROM embeddings)",
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            Ok(count as u64)
        })
        .await
        .unwrap_or(0);

    // Basic memory/CPU estimates using Windows-compatible approaches.
    // We avoid pulling in the heavy `sysinfo` crate. Instead we use
    // simple heuristics from /proc on Linux or best-effort defaults.
    let (cpu_pct, mem_used, mem_total) = get_system_metrics();

    // Disk usage for the brain data directory
    let data_dir = &db.config.data_dir;
    let disk_used_mb = dir_size_mb(data_dir);

    // Ollama availability and models list
    let (ollama_ok, ollama_models) = check_ollama(&db.config.ollama_url).await;

    Ok(SystemHealth {
        cpu_usage_percent: cpu_pct,
        memory_used_mb: mem_used,
        memory_total_mb: mem_total,
        disk_used_mb,
        db_size_mb,
        hnsw_size_mb,
        embedding_queue_size: eq_size,
        uptime_seconds: uptime_seconds(),
        ollama_available: ollama_ok,
        ollama_models,
    })
}

/// Return true if the system is under heavy load (>90% CPU or >90% memory).
/// Used by circuits and the autonomy loop to throttle work.
pub fn should_throttle() -> bool {
    let (cpu, mem_used, mem_total) = get_system_metrics();
    let mem_pct = if mem_total > 0 {
        (mem_used as f32 / mem_total as f32) * 100.0
    } else {
        0.0
    };
    cpu > 90.0 || mem_pct > 90.0
}

// =========================================================================
// Internal helpers
// =========================================================================

/// Lightweight system metrics without the `sysinfo` crate.
/// Returns (cpu_percent, memory_used_mb, memory_total_mb).
fn get_system_metrics() -> (f32, u64, u64) {
    // On Windows we can use GlobalMemoryStatusEx via raw win32, but for
    // simplicity we just read from the process handle.
    #[cfg(target_os = "windows")]
    {
        get_windows_metrics()
    }

    #[cfg(target_os = "linux")]
    {
        get_linux_metrics()
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        (0.0, 0, 0)
    }
}

#[cfg(target_os = "windows")]
fn get_windows_metrics() -> (f32, u64, u64) {
    use std::mem::{size_of, zeroed};

    // MEMORYSTATUSEX via raw FFI — avoids pulling in the `windows` crate.
    #[repr(C)]
    #[allow(non_snake_case)]
    struct MEMORYSTATUSEX {
        dwLength: u32,
        dwMemoryLoad: u32,
        ullTotalPhys: u64,
        ullAvailPhys: u64,
        ullTotalPageFile: u64,
        ullAvailPageFile: u64,
        ullTotalVirtual: u64,
        ullAvailVirtual: u64,
        ullAvailExtendedVirtual: u64,
    }

    extern "system" {
        fn GlobalMemoryStatusEx(lpBuffer: *mut MEMORYSTATUSEX) -> i32;
    }

    unsafe {
        let mut mem: MEMORYSTATUSEX = zeroed();
        mem.dwLength = size_of::<MEMORYSTATUSEX>() as u32;
        if GlobalMemoryStatusEx(&mut mem) != 0 {
            let total_mb = mem.ullTotalPhys / (1024 * 1024);
            let avail_mb = mem.ullAvailPhys / (1024 * 1024);
            let used_mb = total_mb.saturating_sub(avail_mb);
            // dwMemoryLoad is the approximate percentage of physical memory in use
            let cpu_est = mem.dwMemoryLoad as f32 * 0.5; // rough heuristic
            (cpu_est.min(100.0), used_mb, total_mb)
        } else {
            (0.0, 0, 0)
        }
    }
}

#[cfg(target_os = "linux")]
fn get_linux_metrics() -> (f32, u64, u64) {
    // /proc/meminfo for memory
    let mut mem_total: u64 = 0;
    let mut mem_avail: u64 = 0;
    if let Ok(contents) = std::fs::read_to_string("/proc/meminfo") {
        for line in contents.lines() {
            if line.starts_with("MemTotal:") {
                mem_total = parse_proc_kb(line) / 1024;
            } else if line.starts_with("MemAvailable:") {
                mem_avail = parse_proc_kb(line) / 1024;
            }
        }
    }
    let mem_used = mem_total.saturating_sub(mem_avail);

    // /proc/loadavg for CPU (1-min load average / num CPUs)
    let cpu_pct = if let Ok(loadavg) = std::fs::read_to_string("/proc/loadavg") {
        let load_1m: f32 = loadavg
            .split_whitespace()
            .next()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.0);
        let num_cpus = std::thread::available_parallelism()
            .map(|n| n.get() as f32)
            .unwrap_or(1.0);
        ((load_1m / num_cpus) * 100.0).min(100.0)
    } else {
        0.0
    };

    (cpu_pct, mem_used, mem_total)
}

#[cfg(target_os = "linux")]
fn parse_proc_kb(line: &str) -> u64 {
    line.split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0)
}

/// Rough size of a directory tree in MB.
fn dir_size_mb(path: &std::path::Path) -> u64 {
    let mut total: u64 = 0;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let meta = entry.metadata();
            if let Ok(m) = meta {
                if m.is_file() {
                    total += m.len();
                } else if m.is_dir() {
                    total += dir_size_mb(&entry.path());
                }
            }
        }
    }
    total / (1024 * 1024)
}

/// Check Ollama availability and list installed models.
async fn check_ollama(ollama_url: &str) -> (bool, Vec<String>) {
    let url = format!("{}/api/tags", ollama_url.trim_end_matches('/'));
    let client = reqwest::Client::new();
    match client
        .get(&url)
        .timeout(std::time::Duration::from_secs(3))
        .send()
        .await
    {
        Ok(resp) => {
            if resp.status().is_success() {
                if let Ok(body) = resp.json::<serde_json::Value>().await {
                    let models: Vec<String> = body
                        .get("models")
                        .and_then(|m| m.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| {
                                    v.get("name").and_then(|n| n.as_str()).map(String::from)
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    (true, models)
                } else {
                    (true, Vec::new())
                }
            } else {
                (false, Vec::new())
            }
        }
        Err(_) => (false, Vec::new()),
    }
}
