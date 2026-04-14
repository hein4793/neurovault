// Assistant Code synchronization module
// Watches ~/.ai-assistant/projects/ for new chat files and auto-imports them

use crate::db::models::*;
use crate::db::BrainDb;
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::Arc;

/// Start watching Assistant Code directories for new/changed files
pub fn start_file_watcher(db: Arc<BrainDb>) {
    std::thread::spawn(move || {
        let home = match dirs::home_dir() {
            Some(h) => h,
            None => return,
        };

        let watch_dir = home.join(".ai-assistant").join("projects");
        if !watch_dir.exists() {
            log::warn!("Assistant projects directory not found: {:?}", watch_dir);
            return;
        }

        log::info!("Starting file watcher on {:?}", watch_dir);

        let db_clone = db.clone();
        let rt = tokio::runtime::Runtime::new().unwrap();

        let mut watcher = match RecommendedWatcher::new(
            move |result: Result<notify::Event, notify::Error>| {
                if let Ok(event) = result {
                    match event.kind {
                        EventKind::Create(_) | EventKind::Modify(_) => {
                            for path in &event.paths {
                                // Watch for new .jsonl files (chat sessions)
                                if path.extension().map_or(false, |ext| ext == "jsonl")
                                    && !path.to_string_lossy().contains("subagents")
                                {
                                    log::info!("Detected chat file change: {:?}", path);
                                    let db = db_clone.clone();
                                    let path = path.clone();
                                    rt.spawn(async move {
                                        if let Err(e) = auto_import_chat(&db, &path).await {
                                            log::warn!("Auto-import failed: {}", e);
                                        }
                                    });
                                }

                                // Watch for new/changed .md files (memory files)
                                if path.extension().map_or(false, |ext| ext == "md")
                                    && path.to_string_lossy().contains("memory")
                                {
                                    log::info!("Detected memory file change: {:?}", path);
                                    let db = db_clone.clone();
                                    let path = path.clone();
                                    rt.spawn(async move {
                                        if let Err(e) = auto_import_memory_file(&db, &path).await {
                                            log::warn!("Auto-import memory failed: {}", e);
                                        }
                                    });
                                }
                            }
                        }
                        _ => {}
                    }
                }
            },
            Config::default(),
        ) {
            Ok(w) => w,
            Err(e) => {
                log::error!("Failed to create file watcher: {}", e);
                return;
            }
        };

        if let Err(e) = watcher.watch(&watch_dir, RecursiveMode::Recursive) {
            log::error!("Failed to watch directory: {}", e);
            return;
        }

        // Also watch the external-vault
        let vault_dir = home.join(".ai-assistant").join("external-vault");
        if vault_dir.exists() {
            let _ = watcher.watch(&vault_dir, RecursiveMode::Recursive);
        }

        log::info!("File watcher started successfully");

        // Keep the thread alive
        loop {
            std::thread::sleep(std::time::Duration::from_secs(60));
        }
    });
}

async fn auto_import_chat(
    db: &Arc<BrainDb>,
    path: &PathBuf,
) -> Result<(), crate::error::BrainError> {
    let content = std::fs::read_to_string(path)?;

    // Only import the last portion (new messages)
    let lines: Vec<&str> = content.lines().collect();
    let start = if lines.len() > 20 { lines.len() - 20 } else { 0 };

    let mut chunk = String::new();
    for line in &lines[start..] {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
            let msg_type = json["type"].as_str().unwrap_or("");
            if msg_type == "human" || msg_type == "assistant" {
                let role = if msg_type == "human" { "User" } else { "Assistant" };
                if let Some(content_arr) = json["message"]["content"].as_array() {
                    for item in content_arr {
                        if let Some(text) = item["text"].as_str() {
                            if text.len() > 20 {
                                let trimmed = crate::truncate_str(text, 300);
                                chunk += &format!("**{}**: {}\n\n", role, trimmed);
                            }
                        }
                    }
                }
            }
        }
    }

    if chunk.len() > 50 {
        // ===== SYNC STATE CHECK =====
        // Hash the chunk and compare against the last-imported hash for
        // this file. If identical, skip — nothing new to import.
        let chunk_hash = format!("{:x}", Sha256::digest(chunk.as_bytes()));
        let file_key = path.to_string_lossy().to_string();

        #[derive(Debug)]
        struct SyncRow { last_hash: String }
        let fk = file_key.clone();
        let prev: Vec<SyncRow> = db.with_conn(move |conn| -> Result<Vec<SyncRow>, crate::error::BrainError> {
            let mut stmt = conn.prepare(
                "SELECT content_hash FROM sync_state WHERE file_path = ?1 LIMIT 1"
            ).map_err(|e| crate::error::BrainError::Database(e.to_string()))?;
            let rows = stmt.query_map(rusqlite::params![fk], |row| {
                Ok(SyncRow { last_hash: row.get(0)? })
            }).map_err(|e| crate::error::BrainError::Database(e.to_string()))?;
            let mut result = Vec::new();
            for r in rows { if let Ok(s) = r { result.push(s); } }
            Ok(result)
        }).await?;

        if let Some(prev_row) = prev.first() {
            if prev_row.last_hash == chunk_hash {
                // Content hasn't changed since last import — skip
                return Ok(());
            }
        }

        let project_name = path.parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .replace("C--Users-User-OneDrive-Desktop-", "")
            .replace("C--Users-User-", "")
            .replace('-', " ");

        // create_node now has built-in dedup via content_hash check, so
        // even if the sync_state check misses a case, the node won't be
        // duplicated. It'll just bump access_count on the existing one.
        db.create_node(CreateNodeInput {
            title: format!("{} - Live Chat", project_name),
            content: chunk,
            domain: "personal".to_string(),
            topic: project_name.to_lowercase().replace(' ', "-"),
            tags: vec!["chat".to_string(), "auto".to_string(), "live".to_string()],
            node_type: "conversation".to_string(),
            source_type: "auto_sync".to_string(),
            source_url: None,
        }).await?;

        // Update sync_state so next trigger for this file skips if unchanged
        let now = chrono::Utc::now().to_rfc3339();
        let _ = db.with_conn(move |conn| -> Result<(), crate::error::BrainError> {
            conn.execute(
                "INSERT OR REPLACE INTO sync_state (file_path, content_hash, last_synced_at) \
                 VALUES (?1, ?2, ?3)",
                rusqlite::params![file_key, chunk_hash, now],
            ).map_err(|e| crate::error::BrainError::Database(e.to_string()))?;
            Ok(())
        }).await;

        log::info!("Auto-imported chat update from {}", project_name);
    }

    Ok(())
}

async fn auto_import_memory_file(
    db: &Arc<BrainDb>,
    path: &PathBuf,
) -> Result<(), crate::error::BrainError> {
    let content = std::fs::read_to_string(path)?;
    if content.trim().is_empty() { return Ok(()); }

    let title = path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("untitled")
        .replace('_', " ")
        .replace('-', " ");

    db.create_node(CreateNodeInput {
        title: format!("{} (auto-synced)", title),
        content,
        domain: "technology".to_string(),
        topic: title.to_lowercase().replace(' ', "-"),
        tags: vec!["memory".to_string(), "auto".to_string()],
        node_type: "reference".to_string(),
        source_type: "auto_sync".to_string(),
        source_url: None,
    }).await?;

    // Auto-link deferred to autonomy engine (every 60min)
    log::info!("Auto-imported memory file: {}", title);
    Ok(())
}
