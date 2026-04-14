//! BrainCore — Phase 4.2 architectural scaffolding for headless operation.
//!
//! This module extracts the *startup wiring* for all of the brain's
//! background tasks (file watcher, embedding pipeline, autonomy loop,
//! HTTP API, sidekick, master loop, memory tier, fine-tune scheduler,
//! cold storage loop, HNSW loader). Both the Tauri desktop app AND a
//! future headless binary can call `BrainCore::start_all` to get the
//! same set of background services running.
//!
//! ## Why this exists
//!
//! Today the wiring lives inline in `lib.rs::run::setup`. That works for
//! the desktop app but a headless binary (`brain-headless.exe`) couldn't
//! reuse it because it depends on `tauri::AppHandle`. By splitting the
//! Tauri-specific code (event emission, window management) from the
//! Tauri-independent code (database, autonomy, HTTP API, file watcher),
//! we get a clean library that any binary can drive.
//!
//! ## What this module ships TODAY
//!
//! - `BrainCore::start_all(db, app_handle_opt)` — spawns every background
//!   task, optionally taking an AppHandle for event emission
//! - The Tauri setup() in lib.rs still does its own spawning (so this is
//!   additive — it's a refactor scaffold, not a forced cutover)
//!
//! ## What it does NOT ship today
//!
//! - The actual `brain-headless.rs` binary (deferred — requires extracting
//!   Tauri's IPC bridge and replacing it with HTTP-only access)
//! - Service registration logic (use `windows-service` crate when ready)
//! - Headless config separate from desktop config
//!
//! ## Migration path to a true Windows service
//!
//! 1. Copy `lib.rs::run` into `bin/brain-headless.rs`, strip the Tauri
//!    bits, and call `BrainCore::start_all(db, None)` instead of inline
//!    spawning.
//! 2. Add `windows-service = "0.7"` to Cargo.toml
//! 3. Wrap `brain-headless`'s main() in a `windows_service::service_main`
//!    handler that responds to start/stop/restart events.
//! 4. Use `sc.exe create NeuroVault binPath= ...` to register.
//!
//! Total work: ~200 lines of new code, no refactoring needed thanks to
//! this scaffold.

// Phase 4.2 scaffold — not yet wired into either the desktop app or a
// headless binary. Kept here as a single-call entry point for the future
// brain-headless.exe; Tauri's lib.rs::run still spawns tasks inline.
#![allow(dead_code)]
use crate::db::BrainDb;
use std::sync::Arc;

pub struct BrainCore;

impl BrainCore {
    /// Spawn every background task. Pass `Some(app_handle)` from a Tauri
    /// process; pass `None` from a headless binary.
    ///
    /// Returns immediately — the tasks run forever in the background.
    pub fn start_all(db: Arc<BrainDb>, app_handle: Option<tauri::AppHandle>) {
        log::info!("BrainCore::start_all — spawning all background services");

        // === Phase 0/1: file watcher ===
        crate::sync::start_file_watcher(db.clone());

        // === Phase 0/1: embedding pipeline ===
        let emb_db = db.clone();
        let ollama_url = db.config.ollama_url.clone();
        let embedding_model = db.config.embedding_model.clone();
        tauri::async_runtime::spawn(async move {
            crate::embeddings::pipeline::run_embedding_pipeline(
                emb_db,
                ollama_url,
                embedding_model,
            )
            .await;
        });

        // === Phase 0: autonomy loop (needs an AppHandle for event emission) ===
        if let Some(ref handle) = app_handle {
            let auto_db = db.clone();
            let auto_handle = handle.clone();
            tauri::async_runtime::spawn(async move {
                crate::autonomy::run_autonomy_loop(auto_db, auto_handle).await;
            });
        } else {
            log::warn!("BrainCore: no AppHandle — autonomy loop will not be spawned in headless mode (TODO: extract event emission)");
        }

        // === Phase 1.5: HNSW loader + rebuild loop ===
        let hnsw_load_db = db.clone();
        let hnsw_handle = db.hnsw.clone();
        tauri::async_runtime::spawn(async move {
            crate::embeddings::hnsw::load_or_build(hnsw_load_db, hnsw_handle).await;
        });
        let hnsw_rebuild_db = db.clone();
        let hnsw_rebuild_handle = db.hnsw.clone();
        tauri::async_runtime::spawn(async move {
            crate::embeddings::hnsw::rebuild_loop(hnsw_rebuild_db, hnsw_rebuild_handle).await;
        });

        // === Phase 1.1a: HTTP API ===
        let http_db = db.clone();
        tauri::async_runtime::spawn(async move {
            crate::http_api::run_http_server(http_db).await;
        });

        // === Phase 1.2: proactive context injector ===
        let sidekick_db = db.clone();
        tauri::async_runtime::spawn(async move {
            crate::sidekick::run_context_injector(sidekick_db).await;
        });

        // === Phase 2.1: master cognitive loop ===
        let master_db = db.clone();
        tauri::async_runtime::spawn(async move {
            crate::master_loop::run_master_loop(master_db).await;
        });

        // === Phase 2.5: tiered memory loop ===
        let tier_db = db.clone();
        tauri::async_runtime::spawn(async move {
            crate::memory_tier::run_tier_loop(tier_db).await;
        });

        // === Phase 3.2: fine-tune scheduler ===
        let ft_db = db.clone();
        tauri::async_runtime::spawn(async move {
            crate::finetune::run_finetune_scheduler(ft_db).await;
        });

        // === Phase 3.5: cold storage loop ===
        let cs_db = db.clone();
        tauri::async_runtime::spawn(async move {
            crate::cold_storage::run_cold_storage_loop(cs_db).await;
        });

        // === Phase Omega II: swarm orchestrator ===
        let swarm_db = db.clone();
        tauri::async_runtime::spawn(async move {
            crate::swarm::run_swarm_orchestrator(swarm_db).await;
        });

        // === Phase Omega V: data stream poller ===
        let stream_db = db.clone();
        tauri::async_runtime::spawn(async move {
            crate::data_streams::run_stream_poller(stream_db).await;
        });

        log::info!("BrainCore::start_all — all background services spawned");
    }
}
