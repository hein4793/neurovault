//! brain-headless — Phase 3.3 of the master plan.
//!
//! Standalone Windows service binary that runs the brain WITHOUT the
//! Tauri desktop GUI. The same `BrainCore::start_all` function used by
//! the desktop app spawns every background task. The HTTP API on
//! 127.0.0.1:17777 is the only external interface.
//!
//! ## Build
//!
//! ```bash
//! cargo build --features windows-service-mode --bin brain-headless --release
//! ```
//!
//! Output: `target/release/brain-headless.exe`
//!
//! ## Install as a Windows service
//!
//! Open an elevated cmd.exe (Run as administrator):
//!
//! ```cmd
//! sc create ClaudeBrain ^
//!   binPath= "C:\path\to\brain-headless.exe" ^
//!   DisplayName= "NeuroVault" ^
//!   start= auto
//!
//! sc description ClaudeBrain "Personal knowledge brain — autonomy + HTTP API"
//! sc start ClaudeBrain
//! ```
//!
//! Then verify with:
//!
//! ```bash
//! curl http://127.0.0.1:17777/health
//! ```
//!
//! ## Uninstall
//!
//! ```cmd
//! sc stop ClaudeBrain
//! sc delete ClaudeBrain
//! ```
//!
//! ## Why a separate binary?
//!
//! The desktop app needs Tauri's window management, IPC, and frontend
//! bundle. A Windows service:
//! - Has no window
//! - No frontend bundle
//! - No Tauri runtime
//! - Just SurrealDB + autonomy + HTTP API + sidekick + background tasks
//!
//! By splitting into a separate binary we get:
//! - Smaller binary (~30 MB vs ~100 MB for Tauri)
//! - Faster startup (no GUI init)
//! - Can run on a server without a desktop session
//! - Survives user logout (true 24/7 operation)
//!
//! The Tauri desktop app can still run alongside the service. Both share
//! the same database at `~/.neurovault/data/` (well — they
//! can't both write to it at the same time, since SurrealDB is
//! single-process). In practice you run EITHER the desktop app OR the
//! service, never both.

#[cfg(not(feature = "windows-service-mode"))]
fn main() {
    eprintln!("brain-headless requires the 'windows-service-mode' Cargo feature.");
    eprintln!("Rebuild with: cargo build --features windows-service-mode --bin brain-headless");
    std::process::exit(1);
}

#[cfg(feature = "windows-service-mode")]
fn main() -> windows_service::Result<()> {
    use windows_service::service_dispatcher;

    // First arg is the dispatcher entry — Windows passes this when the
    // service starts. If we're invoked directly from the command line
    // (no service control), we run in the foreground for debugging.
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && args[1] == "--foreground" {
        run_foreground();
        return Ok(());
    }

    service_dispatcher::start("ClaudeBrain", ffi_service_main)
}

#[cfg(feature = "windows-service-mode")]
windows_service::define_windows_service!(ffi_service_main, service_main);

#[cfg(feature = "windows-service-mode")]
fn service_main(_arguments: Vec<std::ffi::OsString>) {
    use std::sync::mpsc;
    use std::time::Duration;
    use windows_service::service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
        ServiceType,
    };
    use windows_service::service_control_handler::{self, ServiceControlHandlerResult};

    // Channel for the stop signal from Windows.
    let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>();

    let event_handler = move |control_event| -> ServiceControlHandlerResult {
        match control_event {
            ServiceControl::Stop | ServiceControl::Shutdown => {
                shutdown_tx.send(()).ok();
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    };

    let status_handle = match service_control_handler::register("ClaudeBrain", event_handler) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("Failed to register service control handler: {}", e);
            return;
        }
    };

    // Tell Windows we're starting
    let _ = status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::StartPending,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::from_secs(30),
        process_id: None,
    });

    // Spawn the brain in a tokio runtime
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("Failed to create tokio runtime: {}", e);
            return;
        }
    };

    runtime.spawn(async {
        match claude_brain_lib::headless_main().await {
            Ok(_) => log::info!("brain-headless: started successfully"),
            Err(e) => log::error!("brain-headless: startup failed: {}", e),
        }
    });

    // Tell Windows we're running
    let _ = status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Running,
        controls_accepted: ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    });

    // Wait for the stop signal
    shutdown_rx.recv().ok();

    // Tell Windows we're stopping
    let _ = status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Stopped,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    });
}

/// Foreground mode for development — runs the brain in the current
/// terminal without the Windows service dispatcher.
#[cfg(feature = "windows-service-mode")]
fn run_foreground() {
    env_logger::init();
    log::info!("brain-headless: foreground mode");

    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    runtime.block_on(async {
        if let Err(e) = claude_brain_lib::headless_main().await {
            log::error!("Headless brain failed: {}", e);
        }
        // Run forever
        tokio::signal::ctrl_c().await.ok();
        log::info!("Shutting down on Ctrl+C");
    });
}
