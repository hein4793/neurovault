use std::sync::Arc;
use tauri::Manager;

/// Safely truncate a string to at most `max_bytes` without splitting multi-byte characters.
pub fn truncate_str(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    // Walk backwards from max_bytes to find a valid char boundary
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

mod ai;
mod analysis;
mod anticipatory;
mod attention;
mod autonomy;
mod backup;
mod brain_core;
mod cache;
mod capability_frontier;
mod circuit_performance;
mod circuits;
mod cognitive_fingerprint;
mod cold_storage;
mod cold_storage_parquet;
mod commands;
mod distributed;
mod edge_cache;
mod curiosity_v2;
mod config;
mod context_bundle;
mod context_quality;
mod db;
mod dream_mode;
mod decision_simulator;
mod economics;
mod embeddings;
mod error;
mod events;
mod export;
mod federation;
mod finetune;
mod http_api;
mod ingestion;
mod internal_dialogue;
mod knowledge_compiler;
mod learning;
mod master_loop;
mod mcp;
mod memory_tier;
mod quality;
mod self_model;
mod session_continuity;
mod sidekick;
mod sidekick_suggestions;
mod swarm;
mod sync;
mod system_health;
mod temporal;
mod visual;
mod audio;
mod data_streams;
mod user;
mod vault;
mod world_model;

use db::BrainDb;

/// Phase 3.3 — Headless entry point for the standalone brain-headless
/// binary. Initializes the DB and spawns every background task via
/// `BrainCore::start_all`. Returns once startup is complete; the spawned
/// tasks run forever.
///
/// Called by `bin/brain_headless.rs` when running as a Windows service.
/// The Tauri desktop entry point `run()` does its own inline spawning.
pub async fn headless_main() -> Result<(), String> {
    system_health::mark_start();
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "neurovault=info,warn");
    }
    let _ = env_logger::try_init();

    log::info!("brain-headless: initialising database...");
    let brain_db = BrainDb::init().await.map_err(|e| e.to_string())?;
    let db = std::sync::Arc::new(brain_db);

    log::info!("brain-headless: spawning background tasks via BrainCore...");
    brain_core::BrainCore::start_all(db.clone(), None);

    // Build performance indices in background
    let idx_db = db.clone();
    tauri::async_runtime::spawn(async move {
        idx_db.build_performance_indices().await;
    });

    log::info!("brain-headless: ready");
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    system_health::mark_start();
    // Ensure logs are visible
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "neurovault=info,warn");
    }
    env_logger::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                log::info!("Starting Brain database initialization...");
                match BrainDb::init().await {
                    Ok(brain_db) => {
                        log::info!("Brain database initialized OK");
                        let db = Arc::new(brain_db);
                        // Start file watcher for auto-sync
                        sync::start_file_watcher(db.clone());
                        // Start background embedding pipeline
                        let emb_db = db.clone();
                        let ollama_url = db.config.ollama_url.clone();
                        let embedding_model = db.config.embedding_model.clone();
                        tauri::async_runtime::spawn(async move {
                            embeddings::pipeline::run_embedding_pipeline(
                                emb_db,
                                ollama_url,
                                embedding_model,
                            )
                            .await;
                        });
                        // Auto-export brain knowledge for Claude Code access
                        let export_db = db.clone();
                        tauri::async_runtime::spawn(async move {
                            let export_dir = export_db.config.export_dir();
                            let json_path = export_dir.join("brain-knowledge.json");
                            let nodes_dir = export_dir.join("nodes");
                            match export::export_json(&export_db, &json_path.to_string_lossy()).await {
                                Ok(count) => log::info!("Auto-exported {} nodes to JSON for Claude Code", count),
                                Err(e) => log::warn!("Auto-export JSON failed: {}", e),
                            }
                            match export::export_markdown(&export_db, &nodes_dir.to_string_lossy()).await {
                                Ok(count) => log::info!("Auto-exported {} nodes to markdown for Claude Code", count),
                                Err(e) => log::warn!("Auto-export markdown failed: {}", e),
                            }
                        });
                        // Start autonomy engine (self-improving brain loop)
                        let autonomy_db = db.clone();
                        let autonomy_handle = app_handle.clone();
                        tauri::async_runtime::spawn(async move {
                            autonomy::run_autonomy_loop(autonomy_db, autonomy_handle).await;
                        });

                        // Phase 1.5 — load (or build) the HNSW index in the
                        // background, then run the rebuild loop forever.
                        let hnsw_load_db = db.clone();
                        let hnsw_handle = db.hnsw.clone();
                        tauri::async_runtime::spawn(async move {
                            embeddings::hnsw::load_or_build(hnsw_load_db, hnsw_handle).await;
                        });
                        let hnsw_rebuild_db = db.clone();
                        let hnsw_rebuild_handle = db.hnsw.clone();
                        tauri::async_runtime::spawn(async move {
                            embeddings::hnsw::rebuild_loop(hnsw_rebuild_db, hnsw_rebuild_handle).await;
                        });

                        // Phase 1.1a — HTTP API server for the standalone MCP
                        // bridge. Bound to 127.0.0.1 only.
                        let http_db = db.clone();
                        tauri::async_runtime::spawn(async move {
                            http_api::run_http_server(http_db).await;
                        });

                        // Phase 1.2 — Proactive context injector. Watches the
                        // most recently active Claude Code session and writes
                        // ~/.neurovault/export/active-context.md.
                        // Phase 1.2B upgrade: now event-driven via its own
                        // file watcher with a 30s polling fallback.
                        let sidekick_db = db.clone();
                        tauri::async_runtime::spawn(async move {
                            sidekick::run_context_injector(sidekick_db).await;
                        });

                        // Phase 1.2C — Suggestion engine. Runs alongside the
                        // context injector and produces categorized
                        // insights/warnings/optimizations every ~60s.
                        let suggestions_db = db.clone();
                        tauri::async_runtime::spawn(async move {
                            sidekick_suggestions::run_suggestion_engine(suggestions_db).await;
                        });

                        // Phase 2.1 — Master cognitive loop (observe → analyze
                        // → improve → act). Runs every 30 minutes as a higher-
                        // order monitor over the entire brain.
                        let master_db = db.clone();
                        tauri::async_runtime::spawn(async move {
                            master_loop::run_master_loop(master_db).await;
                        });

                        // Phase 2.5 — Tiered memory promotion. Walks all nodes
                        // and stamps memory_tier (hot/warm/cold) based on
                        // access recency. Runs every 6 hours.
                        let tier_db = db.clone();
                        tauri::async_runtime::spawn(async move {
                            memory_tier::run_tier_loop(tier_db).await;
                        });

                        // Phase 3.2 — Fine-tuning automation scheduler.
                        // Daily check, recommends a retrain when the dataset
                        // grows ≥30% or 30+ days have passed.
                        let ft_db = db.clone();
                        tauri::async_runtime::spawn(async move {
                            finetune::run_finetune_scheduler(ft_db).await;
                        });

                        // Phase 3.5 — Cold storage archival loop.
                        // Weekly export of memory_tier='cold' nodes to a
                        // versioned JSONL file. Never deletes from DB.
                        let cs_db = db.clone();
                        tauri::async_runtime::spawn(async move {
                            cold_storage::run_cold_storage_loop(cs_db).await;
                        });

                        // Phase Omega II — Swarm orchestrator.
                        // Multi-agent task decomposition and execution loop
                        // that runs every 5 minutes.
                        let swarm_db = db.clone();
                        tauri::async_runtime::spawn(async move {
                            swarm::run_swarm_orchestrator(swarm_db).await;
                        });

                        // Phase Omega V — Data stream poller.
                        // Polls registered RSS/API streams on their configured
                        // intervals. Checks every 5 minutes.
                        let stream_db = db.clone();
                        tauri::async_runtime::spawn(async move {
                            data_streams::run_stream_poller(stream_db).await;
                        });

                        app_handle.manage(db.clone());
                        log::info!("Brain database initialized successfully");

                        // Build performance indices in background (don't block app)
                        let idx_db = db.clone();
                        tauri::async_runtime::spawn(async move {
                            idx_db.build_performance_indices().await;
                        });
                    }
                    Err(e) => {
                        log::error!("Failed to initialize brain database: {}", e);
                    }
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Graph
            commands::graph::get_all_nodes,
            commands::graph::get_all_edges,
            commands::graph::create_node,
            commands::graph::update_node,
            commands::graph::delete_node,
            commands::graph::create_edge,
            commands::graph::delete_edge,
            commands::graph::get_edges_for_node,
            commands::graph::get_node_count,
            commands::graph::get_edge_count,
            commands::graph::get_nodes_paginated,
            commands::graph::get_node_cloud,
            commands::graph::get_domain_clusters,
            commands::graph::get_edge_bundle_counts,
            // Search
            commands::search::search_nodes,
            commands::search::semantic_search,
            // Ingestion
            commands::ingest::ingest_url,
            commands::ingest::ingest_text,
            commands::ingest::import_claude_memory,
            commands::ingest::import_chat_history,
            commands::ingest::research_topic,
            commands::ingest::research_batch,
            commands::ingest::ingest_files,
            commands::ingest::ingest_project_directory,
            // Embeddings
            commands::embeddings::find_similar_nodes,
            commands::embeddings::get_embedding_stats,
            commands::embeddings::get_embedding_status,
            commands::embeddings::generate_embeddings,
            commands::embeddings::scan_duplicates,
            commands::embeddings::semantic_search_v2,
            // Stats
            commands::stats::get_brain_stats,
            commands::stats::auto_link_nodes,
            // Settings
            commands::settings::get_settings,
            commands::settings::update_settings,
            commands::settings::clear_cache,
            commands::settings::get_brain_version,
            // Quality
            commands::quality::calculate_quality,
            commands::quality::calculate_decay,
            commands::quality::get_quality_report,
            commands::quality::merge_duplicate_nodes,
            commands::quality::enhance_quality,
            commands::quality::boost_iq,
            commands::quality::deep_learn,
            commands::quality::quality_sweep,
            commands::quality::maximize_iq,
            // AI
            commands::ai::ask_brain,
            commands::ai::summarize_node_ai,
            commands::ai::backfill_summaries,
            commands::ai::extract_tags_ai,
            commands::ai::simulate_decision,
            commands::ai::run_dialogue,
            commands::ai::get_cognitive_fingerprint,
            commands::ai::synthesize_cognitive_fingerprint,
            // Learning
            commands::learning::get_knowledge_gaps,
            commands::learning::get_curiosity_queue,
            commands::learning::create_research_mission,
            commands::learning::get_research_missions,
            // Analysis
            commands::analysis::analyze_patterns,
            commands::analysis::analyze_trends,
            commands::analysis::get_recommendations,
            // Autonomy
            commands::autonomy::get_autonomy_status,
            commands::autonomy::set_autonomy_enabled,
            commands::autonomy::trigger_autonomy_task,
            // User
            commands::user::get_user_profile,
            commands::user::synthesize_user_profile,
            // Phase 3.1 — installed Ollama models for the multi-model UI
            commands::models::list_installed_models,
            // Phase 4.1 — multi-brain support
            commands::brains::list_brains,
            commands::brains::create_brain,
            commands::brains::delete_brain,
            commands::brains::get_active_brain,
            commands::brains::set_active_brain,
            commands::brains::get_brain_stats_for,
            // Phase 4.7 — brain activity panel
            commands::activity::get_brain_activity,
            // Phase 1.2C — sidekick suggestions
            commands::activity::get_active_suggestions,
            // MCP — Phase 0 brain bridge for Claude Code
            commands::mcp::get_mcp_status,
            commands::mcp::brain_recall,
            commands::mcp::brain_context,
            commands::mcp::brain_preferences,
            commands::mcp::brain_decisions,
            commands::mcp::brain_learn,
            // Phase 4.9 — additional MCP brain tools
            commands::mcp::brain_critique,
            commands::mcp::brain_history,
            commands::mcp::brain_export_subgraph,
            commands::mcp::brain_plan,
            // Backup & Export
            commands::backup::create_backup,
            commands::backup::list_backups,
            commands::backup::restore_backup,
            commands::backup::export_json,
            commands::backup::export_markdown,
            commands::backup::export_csv,
            commands::backup::generate_training_dataset,
            commands::backup::export_personal_training,
            commands::backup::list_cold_archives,
            commands::backup::import_cold_archive,
            commands::backup::run_cold_storage_pass,
            commands::backup::run_cold_storage_pass_parquet,
            commands::backup::run_finetune_now,
            commands::backup::cold_archive_token,
            commands::backup::purge_cold_archive,
            // Database repair + compaction
            commands::repair::scan_corrupted_nodes,
            commands::repair::scan_corrupted_edges,
            commands::repair::repair_delete_corrupted,
            commands::compact::compact_export_all,
            commands::compact::compact_import_all,
            // Phase Omega II — Swarm orchestrator
            commands::swarm::get_swarm_status,
            commands::swarm::create_swarm_task,
            commands::swarm::decompose_goal,
            commands::swarm::get_swarm_tasks,
            // Phase Omega IV — Recursive self-improvement
            commands::self_improve::get_knowledge_rules,
            commands::self_improve::get_circuit_performance,
            commands::self_improve::get_capabilities,
            commands::self_improve::compile_rules_now,
            // Phase Omega III — World Model
            commands::world::get_world_entities,
            commands::world::get_causal_links,
            commands::world::simulate_scenario_cmd,
            commands::world::get_predictions,
            // Phase Omega IX — Consciousness Layer
            commands::consciousness::get_self_model,
            commands::consciousness::get_attention_window,
            commands::consciousness::get_curiosity_targets_v2,
            // Phase Omega VII — Infrastructure
            commands::infrastructure::get_cluster_status,
            commands::infrastructure::register_brain_node,
            commands::infrastructure::get_edge_devices,
            commands::infrastructure::register_edge_device,
            commands::infrastructure::compute_edge_cache_cmd,
            commands::infrastructure::export_edge_cache_cmd,
            commands::infrastructure::get_system_health,
            // Phase Omega VI — Federation (The Collective)
            commands::federation::get_federation_status,
            commands::federation::register_federated_brain,
            commands::federation::share_knowledge_cmd,
            commands::federation::sync_with_brain_cmd,
            // Phase Omega VIII — Economic Autonomy
            commands::economics::record_revenue,
            commands::economics::record_cost,
            commands::economics::get_economic_report,
            // Phase Omega V — Sensory Expansion
            commands::sensory::analyze_image,
            commands::sensory::analyze_screenshot,
            commands::sensory::ingest_diagram,
            commands::sensory::transcribe_audio,
            commands::sensory::ingest_voice_note,
            commands::sensory::add_data_stream,
            commands::sensory::get_data_streams,
            commands::sensory::poll_streams_now,
        ])
        .run(tauri::generate_context!())
        .expect("error while running NeuroVault");
}
