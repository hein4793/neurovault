//! Phase 4 — Adaptive power policy.
//!
//! A tiny state machine that picks one of five operating modes based on
//! live signals (AC vs battery, user presence — more inputs hook up here
//! later). The mode is read by the LLM factory in `commands/ai.rs` to
//! decide whether to demote GPU calls to CPU or queue them entirely.
//!
//! Design notes:
//! - Mode is stored in an `AtomicU8` so every inference-path read is
//!   one atomic load with Relaxed ordering. No contention.
//! - A single background task polls AC status every 30s. It's cheap
//!   (a single kernel32 call) and that cadence is fine — the user can't
//!   tell the difference between a 0s and 30s mode transition.
//! - Battery detection on Windows is a direct FFI call into
//!   `kernel32::GetSystemPowerStatus`. No new crate deps.
//! - On non-Windows, the detector returns `on_battery = false` so the
//!   policy stays in Normal mode — safe default.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::db::BrainDb;

// =========================================================================
// Mode
// =========================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PowerMode {
    /// Plugged in, user active — default routing (GPU for interactive,
    /// CPU for batch when Phase 3 config permits).
    Normal = 0,
    /// On battery or user-requested eco — everything the router would
    /// normally send to GPU gets demoted to CPU; true batch jobs queue.
    Eco = 1,
    /// Plugged in, user idle — batch circuits may burst through backlog
    /// on GPU to catch up. Reserved for Phase 4.1.
    IdleOpportunistic = 2,
    /// GPU temperature / CPU load too high — downshift one tier.
    /// Reserved (thermal sensors need vendor SDKs we haven't wired).
    ThermalThrottle = 3,
    /// UPS below critical threshold — only Interactive circuits run,
    /// everything else queues until mains returns.
    LoadShed = 4,
}

impl PowerMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Eco => "eco",
            Self::IdleOpportunistic => "idle_opportunistic",
            Self::ThermalThrottle => "thermal_throttle",
            Self::LoadShed => "load_shed",
        }
    }

    fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Eco,
            2 => Self::IdleOpportunistic,
            3 => Self::ThermalThrottle,
            4 => Self::LoadShed,
            _ => Self::Normal,
        }
    }
}

static POWER_MODE: AtomicU8 = AtomicU8::new(PowerMode::Normal as u8);

pub fn current_mode() -> PowerMode {
    PowerMode::from_u8(POWER_MODE.load(Ordering::Relaxed))
}

pub fn set_mode(m: PowerMode) {
    let prev = current_mode();
    POWER_MODE.store(m as u8, Ordering::Relaxed);
    if prev != m {
        log::info!("power_policy: mode transition {} -> {}", prev.as_str(), m.as_str());
    }
}

/// Should the router demote GPU-eligible calls to CPU right now?
/// Read by `commands/ai.rs::build_llm_client_for` before every routing
/// decision.
pub fn prefer_cpu() -> bool {
    matches!(current_mode(), PowerMode::Eco | PowerMode::LoadShed)
}

/// Should batch circuits queue (refuse to run) right now?
/// Reserved for the `LoadShed` mode which will be auto-triggered once
/// UPS/battery integration wires in. The caller is the batch scheduler
/// (not yet wired), so this function stays unused until then.
#[allow(dead_code)]
pub fn should_queue_batch() -> bool {
    matches!(current_mode(), PowerMode::LoadShed)
}

// =========================================================================
// Signal: AC vs battery
// =========================================================================

#[cfg(target_os = "windows")]
#[repr(C)]
#[allow(non_snake_case)]
struct SystemPowerStatus {
    ACLineStatus: u8,
    BatteryFlag: u8,
    BatteryLifePercent: u8,
    SystemStatusFlag: u8,
    BatteryLifeTime: u32,
    BatteryFullLifeTime: u32,
}

#[cfg(target_os = "windows")]
#[link(name = "kernel32")]
extern "system" {
    fn GetSystemPowerStatus(lpSystemPowerStatus: *mut SystemPowerStatus) -> i32;
}

/// Read AC line status. `Some(true)` = on battery, `Some(false)` = plugged in,
/// `None` = unknown (non-Windows or API failure).
pub fn on_battery() -> Option<bool> {
    #[cfg(target_os = "windows")]
    unsafe {
        let mut status: SystemPowerStatus = std::mem::zeroed();
        if GetSystemPowerStatus(&mut status) != 0 {
            // 0 = offline (battery), 1 = online (AC), 255 = unknown
            return match status.ACLineStatus {
                0 => Some(true),
                1 => Some(false),
                _ => None,
            };
        }
        None
    }

    #[cfg(not(target_os = "windows"))]
    None
}

// =========================================================================
// Policy loop
// =========================================================================

/// Poll power signals every 30s and update `POWER_MODE`. Kept simple:
/// right now only AC detection drives transitions. Additional inputs
/// (thermal, UPS, user presence) can slot in without changing the
/// public API.
pub async fn run_power_policy_loop(_db: Arc<BrainDb>) {
    log::info!("power_policy: starting (30s cadence)");
    loop {
        tokio::time::sleep(Duration::from_secs(30)).await;

        let desired = match on_battery() {
            Some(true) => PowerMode::Eco,
            Some(false) => PowerMode::Normal,
            None => PowerMode::Normal, // unknown → safe default
        };
        set_mode(desired);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_roundtrip() {
        assert_eq!(PowerMode::from_u8(0), PowerMode::Normal);
        assert_eq!(PowerMode::from_u8(1), PowerMode::Eco);
        assert_eq!(PowerMode::from_u8(4), PowerMode::LoadShed);
        assert_eq!(PowerMode::from_u8(99), PowerMode::Normal);
    }

    #[test]
    fn prefer_cpu_respects_mode() {
        set_mode(PowerMode::Normal);
        assert!(!prefer_cpu());
        set_mode(PowerMode::Eco);
        assert!(prefer_cpu());
        set_mode(PowerMode::LoadShed);
        assert!(prefer_cpu());
        set_mode(PowerMode::Normal);
    }
}
