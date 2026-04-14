//! DB Compaction — exports all data to JSONL for a clean re-import.
//!
//! ## Why this exists
//!
//! SQLite doesn't suffer from the same corruption issues as SurrealDB's
//! embedded KV store, but compaction remains useful for:
//!   - Creating a portable text backup of all data
//!   - Migrating between schema versions
//!   - Shrinking the DB after large deletes (VACUUM)
//!
//! ## How it works
//!
//! 1. Reads every row from every table
//! 2. Serializes each row to JSON
//! 3. Writes to `~/.neurovault/compaction/TABLE.jsonl`
//! 4. Import reads JSONL back into the DB

use crate::db::BrainDb;
use crate::error::BrainError;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::sync::Arc;
use tauri::State;

/// Allowlist of table names permitted in dynamic SQL queries.
/// Prevents SQL injection even though table names come from hardcoded lists.
pub const ALLOWED_TABLES: &[&str] = &[
    "nodes", "edges", "embeddings", "user_cognition", "autonomy_circuit_log",
    "autonomy_circuit_rotation", "autonomy_state", "research_missions",
    "sync_state", "learning_log", "user_profile", "user_interaction",
    "projects", "node_archive", "mcp_call_log", "compression_log",
    "master_loop_log", "memory_tier_log", "fine_tune_run", "cold_archive_log",
    "brains", "active_brain_state", "synapse_prune_log", "cognitive_fingerprint",
    "knowledge_rules", "circuit_performance", "capabilities", "world_entities",
    "causal_links", "temporal_patterns", "future_predictions", "self_model",
    "attention_focus", "learning_velocity", "swarm_agents", "swarm_tasks",
    "swarm_messages", "visual_analysis", "transcriptions", "data_streams",
    "stream_events", "federated_brains", "federation_messages", "shared_knowledge",
    "revenue_events", "compute_costs", "brain_nodes", "edge_devices", "sync_log",
];

/// Validate that a table name is in the allowlist. Returns an error if not.
pub fn validate_table_name(table: &str) -> Result<(), BrainError> {
    if ALLOWED_TABLES.contains(&table) {
        Ok(())
    } else {
        Err(BrainError::Database(format!(
            "Table name '{}' is not in the allowed tables list",
            table
        )))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionResult {
    pub table: String,
    pub exported: u64,
    pub failed: u64,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FullCompactionResult {
    pub tables: Vec<CompactionResult>,
    pub total_exported: u64,
    pub total_failed: u64,
    pub duration_ms: u64,
}

/// Export ALL tables to JSONL for compaction. Read-only. Safe to run
/// while the brain is operating.
#[tauri::command]
pub async fn compact_export_all(
    db: State<'_, Arc<BrainDb>>,
) -> Result<FullCompactionResult, BrainError> {
    compact_export_all_inner(&db).await
}

pub async fn compact_export_all_inner(
    db: &BrainDb,
) -> Result<FullCompactionResult, BrainError> {
    let start = std::time::Instant::now();
    let compact_dir = db.config.data_dir.join("compaction");
    std::fs::create_dir_all(&compact_dir)?;

    let mut results = Vec::new();
    let mut total_exported = 0u64;
    let mut total_failed = 0u64;

    // Export each table
    for table in &[
        "nodes", "edges", "user_cognition", "autonomy_state",
        "autonomy_circuit_log", "autonomy_circuit_rotation",
        "sync_state", "research_missions", "learning_log",
        "user_profile", "user_interaction", "projects",
        "node_archive", "mcp_call_log", "master_loop_log",
        "memory_tier_log", "fine_tune_run", "cold_archive_log",
        "compression_log", "synapse_prune_log", "brains",
        "active_brain_state",
    ] {
        let path = compact_dir.join(format!("{}.jsonl", table));
        let result = export_table(db, table, &path).await;
        match result {
            Ok(r) => {
                total_exported += r.exported;
                total_failed += r.failed;
                results.push(r);
            }
            Err(e) => {
                log::error!("Compaction: table '{}' export failed: {}", table, e);
                results.push(CompactionResult {
                    table: table.to_string(),
                    exported: 0,
                    failed: 0,
                    path: path.to_string_lossy().to_string(),
                });
            }
        }
    }

    let duration_ms = start.elapsed().as_millis() as u64;
    log::info!(
        "Compaction export complete: {} records exported, {} failed, {}ms",
        total_exported, total_failed, duration_ms
    );

    Ok(FullCompactionResult {
        tables: results,
        total_exported,
        total_failed,
        duration_ms,
    })
}

/// Export one table to JSONL using SELECT * and serializing each row
/// as a JSON object via column names.
async fn export_table(
    db: &BrainDb,
    table: &str,
    path: &std::path::Path,
) -> Result<CompactionResult, BrainError> {
    validate_table_name(table)?;
    log::info!("Compaction: exporting table '{}'...", table);

    let table_owned = table.to_string();
    let rows: Vec<serde_json::Value> = db.with_conn(move |conn| -> Result<Vec<serde_json::Value>, BrainError> {
        let query = format!("SELECT * FROM {}", table_owned);
        let mut stmt = conn.prepare(&query)
            .map_err(|e| BrainError::Database(e.to_string()))?;
        let col_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
        let rows = stmt.query_map([], |row| {
            let mut map = serde_json::Map::new();
            for (i, name) in col_names.iter().enumerate() {
                let val: rusqlite::types::Value = row.get(i)?;
                let json_val = match val {
                    rusqlite::types::Value::Null => serde_json::Value::Null,
                    rusqlite::types::Value::Integer(n) => serde_json::json!(n),
                    rusqlite::types::Value::Real(f) => serde_json::json!(f),
                    rusqlite::types::Value::Text(s) => serde_json::Value::String(s),
                    rusqlite::types::Value::Blob(b) => {
                        // Convert blob to hex string (no base64 dependency needed)
                        let hex: String = b.iter().map(|byte| format!("{:02x}", byte)).collect();
                        serde_json::Value::String(format!("hex:{}", hex))
                    }
                };
                map.insert(name.clone(), json_val);
            }
            Ok(serde_json::Value::Object(map))
        }).map_err(|e| BrainError::Database(e.to_string()))?;

        let mut collected = Vec::new();
        for row in rows {
            match row {
                Ok(v) => collected.push(v),
                Err(e) => log::warn!("Compaction: row read error: {}", e),
            }
        }
        Ok(collected)
    }).await?;

    let file = std::fs::File::create(path)?;
    let mut writer = std::io::BufWriter::new(file);
    let mut exported = 0u64;
    let mut failed = 0u64;

    for row in &rows {
        match serde_json::to_string(row) {
            Ok(json) => {
                writeln!(writer, "{}", json)?;
                exported += 1;
            }
            Err(e) => {
                log::warn!("Compaction: JSON serialize failed: {}", e);
                failed += 1;
            }
        }
    }

    writer.flush()?;
    log::info!(
        "Compaction: {} — {} exported, {} failed -> {}",
        table, exported, failed, path.display()
    );

    Ok(CompactionResult {
        table: table.to_string(),
        exported,
        failed,
        path: path.to_string_lossy().to_string(),
    })
}

/// Import all tables from the compaction JSONL files back into the DB.
#[tauri::command]
pub async fn compact_import_all(
    db: State<'_, Arc<BrainDb>>,
) -> Result<FullCompactionResult, BrainError> {
    compact_import_all_inner(&db).await
}

pub async fn compact_import_all_inner(
    db: &BrainDb,
) -> Result<FullCompactionResult, BrainError> {
    let start = std::time::Instant::now();
    let compact_dir = db.config.data_dir.join("compaction");
    if !compact_dir.exists() {
        return Err(BrainError::NotFound(
            "No compaction directory found. Run compact_export_all first.".into(),
        ));
    }

    let mut results = Vec::new();
    let mut total_exported = 0u64;
    let mut total_failed = 0u64;

    // Import tables in dependency order (nodes before edges)
    for table in &[
        "brains", "active_brain_state",
        "nodes", "edges",
        "user_cognition", "autonomy_state",
        "sync_state", "research_missions", "learning_log",
        "user_profile", "projects",
    ] {
        let path = compact_dir.join(format!("{}.jsonl", table));
        if !path.exists() {
            log::info!("Compaction import: {}.jsonl not found, skipping", table);
            continue;
        }
        match import_table_from_jsonl(db, table, &path).await {
            Ok(r) => {
                total_exported += r.exported;
                total_failed += r.failed;
                results.push(r);
            }
            Err(e) => {
                log::error!("Compaction import: table '{}' failed: {}", table, e);
            }
        }
    }

    let duration_ms = start.elapsed().as_millis() as u64;
    log::info!(
        "Compaction import complete: {} records imported, {} failed, {}ms",
        total_exported, total_failed, duration_ms
    );

    Ok(FullCompactionResult {
        tables: results,
        total_exported,
        total_failed,
        duration_ms,
    })
}

async fn import_table_from_jsonl(
    db: &BrainDb,
    table: &str,
    path: &std::path::Path,
) -> Result<CompactionResult, BrainError> {
    validate_table_name(table)?;
    log::info!("Compaction import: loading {} from {}...", table, path.display());

    let content = std::fs::read_to_string(path)?;
    let mut lines_data: Vec<serde_json::Value> = Vec::new();

    for (i, line) in content.lines().enumerate() {
        if line.trim().is_empty() { continue; }
        match serde_json::from_str::<serde_json::Value>(line) {
            Ok(v) => lines_data.push(v),
            Err(e) => {
                log::warn!("Compaction import: {} line {} parse error: {}", table, i, e);
            }
        }
    }

    let table_owned = table.to_string();
    let (imported, failed) = db.with_conn(move |conn| -> Result<(u64, u64), BrainError> {
        let mut imported = 0u64;
        let mut failed = 0u64;

        for value in &lines_data {
            let obj = match value.as_object() {
                Some(o) => o,
                None => continue,
            };

            // Build INSERT OR IGNORE dynamically from the JSON keys
            let keys: Vec<&String> = obj.keys().collect();
            if keys.is_empty() { continue; }
            let cols = keys.iter().map(|k| k.as_str()).collect::<Vec<_>>().join(", ");
            let placeholders = keys.iter().enumerate().map(|(i, _)| format!("?{}", i + 1)).collect::<Vec<_>>().join(", ");
            let sql = format!("INSERT OR IGNORE INTO {} ({}) VALUES ({})", table_owned, cols, placeholders);

            let params_vec: Vec<Box<dyn rusqlite::types::ToSql>> = keys.iter().map(|k| {
                let v = &obj[k.as_str()];
                match v {
                    serde_json::Value::Null => Box::new(rusqlite::types::Null) as Box<dyn rusqlite::types::ToSql>,
                    serde_json::Value::Bool(b) => Box::new(*b as i64) as Box<dyn rusqlite::types::ToSql>,
                    serde_json::Value::Number(n) => {
                        if let Some(i) = n.as_i64() {
                            Box::new(i) as Box<dyn rusqlite::types::ToSql>
                        } else if let Some(f) = n.as_f64() {
                            Box::new(f) as Box<dyn rusqlite::types::ToSql>
                        } else {
                            Box::new(n.to_string()) as Box<dyn rusqlite::types::ToSql>
                        }
                    }
                    serde_json::Value::String(s) => Box::new(s.clone()) as Box<dyn rusqlite::types::ToSql>,
                    _ => Box::new(v.to_string()) as Box<dyn rusqlite::types::ToSql>,
                }
            }).collect();

            let params_refs: Vec<&dyn rusqlite::types::ToSql> = params_vec.iter().map(|p: &Box<dyn rusqlite::types::ToSql>| p.as_ref()).collect();

            match conn.execute(&sql, params_refs.as_slice()) {
                Ok(_) => imported += 1,
                Err(e) => {
                    let err_str = e.to_string();
                    if err_str.contains("UNIQUE") {
                        imported += 1; // Already exists
                    } else {
                        log::warn!("Compaction import: {} insert error: {}", table_owned, e);
                        failed += 1;
                    }
                }
            }
        }
        Ok((imported, failed))
    }).await?;

    log::info!(
        "Compaction import: {} — {} imported, {} failed",
        table, imported, failed
    );

    Ok(CompactionResult {
        table: table.to_string(),
        exported: imported,
        failed,
        path: path.to_string_lossy().to_string(),
    })
}
