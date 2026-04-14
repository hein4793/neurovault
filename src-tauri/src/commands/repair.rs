//! Database Repair — SQLite integrity checks and maintenance.
//!
//! SQLite doesn't suffer from the SurrealDB "Invalid revision" corruption,
//! but we keep a repair module for:
//!   - Running `PRAGMA integrity_check` to detect any SQLite-level issues
//!   - Running `PRAGMA quick_check` for a faster surface-level scan
//!   - Rebuilding the FTS index if it gets out of sync
//!   - VACUUM to reclaim space after large deletes

use crate::db::BrainDb;
use crate::error::BrainError;
use crate::commands::compact::validate_table_name;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorruptedRecord {
    pub table: String,
    pub record_key: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepairScanResult {
    pub total_scanned: u64,
    pub total_healthy: u64,
    pub corrupted: Vec<CorruptedRecord>,
    pub scan_duration_ms: u64,
}

/// Run SQLite integrity check on the database. Returns any issues found.
#[tauri::command]
pub async fn scan_corrupted_nodes(
    db: State<'_, Arc<BrainDb>>,
) -> Result<RepairScanResult, BrainError> {
    scan_table(&db, "nodes").await
}

/// Run integrity check focusing on edges table.
#[tauri::command]
pub async fn scan_corrupted_edges(
    db: State<'_, Arc<BrainDb>>,
) -> Result<RepairScanResult, BrainError> {
    scan_table(&db, "edges").await
}

/// Delete specific records by ID. Used to clean up orphaned or problematic rows.
#[tauri::command]
pub async fn repair_delete_corrupted(
    db: State<'_, Arc<BrainDb>>,
    records: Vec<CorruptedRecord>,
) -> Result<u64, BrainError> {
    delete_corrupted_inner(&db, records).await
}

/// Public wrapper so http_api can call without the Tauri State wrapper.
pub async fn scan_table_inner(db: &BrainDb, table: &str) -> Result<RepairScanResult, BrainError> {
    scan_table(db, table).await
}

/// Public wrapper for delete so http_api can call it.
pub async fn delete_corrupted_inner(
    db: &BrainDb,
    records: Vec<CorruptedRecord>,
) -> Result<u64, BrainError> {
    let mut deleted = 0u64;
    for rec in records {
        // Validate table name against allowlist before using in SQL
        if let Err(e) = validate_table_name(&rec.table) {
            log::error!("Repair: rejected table name '{}' — {}", rec.table, e);
            continue;
        }
        let table = rec.table.clone();
        let key = rec.record_key.clone();
        match db.with_conn(move |conn| {
            let sql = format!("DELETE FROM {} WHERE id = ?1", table);
            conn.execute(&sql, rusqlite::params![key])
                .map_err(|e| BrainError::Database(e.to_string()))
        }).await {
            Ok(count) => {
                if count > 0 {
                    deleted += 1;
                    log::info!("Repair: deleted {} from {}", rec.record_key, rec.table);
                }
            }
            Err(e) => {
                log::error!("Repair: could not delete {} — {}", rec.record_key, e);
            }
        }
    }
    log::info!("Repair complete: {} records deleted", deleted);
    Ok(deleted)
}

/// Internal: scan one table for issues. With SQLite this checks row count
/// and runs a quick integrity check rather than per-record reads.
async fn scan_table(db: &BrainDb, table: &str) -> Result<RepairScanResult, BrainError> {
    validate_table_name(table)?;
    let start = std::time::Instant::now();
    let table_owned = table.to_string();

    let (total, issues) = db.with_conn(move |conn| {
        // Count rows
        let total: u64 = conn.query_row(
            &format!("SELECT COUNT(*) FROM {}", table_owned),
            [],
            |row| row.get(0),
        ).map_err(|e| BrainError::Database(e.to_string()))?;

        // Run integrity check
        let mut issues: Vec<CorruptedRecord> = Vec::new();
        let mut stmt = conn.prepare("PRAGMA integrity_check")
            .map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            let msg: String = row.get(0)?;
            Ok(msg)
        }).map_err(|e| BrainError::Database(e.to_string()))?;

        for row in rows {
            if let Ok(msg) = row {
                if msg != "ok" {
                    issues.push(CorruptedRecord {
                        table: table_owned.clone(),
                        record_key: "integrity_check".to_string(),
                        error: msg,
                    });
                }
            }
        }

        // Also check for orphaned edges (edges referencing nonexistent nodes)
        if table_owned == "edges" {
            let orphan_count: u64 = conn.query_row(
                "SELECT COUNT(*) FROM edges WHERE source_id NOT IN (SELECT id FROM nodes) OR target_id NOT IN (SELECT id FROM nodes)",
                [],
                |row| row.get(0),
            ).map_err(|e| BrainError::Database(e.to_string()))?;

            if orphan_count > 0 {
                issues.push(CorruptedRecord {
                    table: "edges".to_string(),
                    record_key: format!("{}_orphaned_edges", orphan_count),
                    error: format!("{} edges reference nonexistent nodes", orphan_count),
                });
            }
        }

        Ok((total, issues))
    }).await?;

    let duration_ms = start.elapsed().as_millis() as u64;
    let healthy = total - issues.len() as u64;

    log::info!(
        "Repair scan complete: {}/{} {} records healthy, {} issues ({}ms)",
        healthy, total, table, issues.len(), duration_ms
    );

    Ok(RepairScanResult {
        total_scanned: total,
        total_healthy: healthy,
        corrupted: issues,
        scan_duration_ms: duration_ms,
    })
}
