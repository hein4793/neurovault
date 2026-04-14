//! Phase Omega Part VII — Edge Computing Cache
//!
//! Lightweight cache layer for edge devices (phone, tablet, laptop).
//! Selects the highest-attention nodes from the brain and exports them
//! as a compact JSON bundle that can be synced to constrained devices.

use crate::db::BrainDb;
use crate::error::BrainError;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// =========================================================================
// Models
// =========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeDevice {
    pub device_id: String,
    pub device_name: String,
    pub cache_size: u64,
    pub cached_node_ids: Vec<String>,
    pub last_synced: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeCacheExport {
    pub device_id: String,
    pub device_name: String,
    pub nodes: Vec<serde_json::Value>,
    pub exported_at: String,
}

// =========================================================================
// Schema initialisation
// =========================================================================

pub async fn init_edge_cache(db: &Arc<BrainDb>) -> Result<(), BrainError> {
    db.with_conn(|conn| {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS edge_devices (
                device_id TEXT PRIMARY KEY,
                device_name TEXT NOT NULL,
                cache_size INTEGER NOT NULL DEFAULT 1000,
                cached_node_ids TEXT NOT NULL DEFAULT '[]',
                last_synced TEXT,
                created_at TEXT NOT NULL
            );
            ",
        )
        .map_err(|e| BrainError::Database(format!("edge_cache schema init: {}", e)))?;
        Ok(())
    })
    .await
}

// =========================================================================
// Core operations
// =========================================================================

/// Register a new edge device.
pub async fn register_edge_device(
    db: &Arc<BrainDb>,
    device_name: String,
    cache_size: u64,
) -> Result<EdgeDevice, BrainError> {
    init_edge_cache(db).await?;
    let device_id = format!("edge:{}", uuid::Uuid::now_v7());
    let now = chrono::Utc::now().to_rfc3339();
    let device = EdgeDevice {
        device_id: device_id.clone(),
        device_name,
        cache_size,
        cached_node_ids: Vec::new(),
        last_synced: None,
        created_at: now.clone(),
    };
    let d = device.clone();
    db.with_conn(move |conn| {
        conn.execute(
            "INSERT INTO edge_devices (device_id, device_name, cache_size, cached_node_ids, last_synced, created_at)
             VALUES (?1, ?2, ?3, '[]', NULL, ?4)",
            params![d.device_id, d.device_name, d.cache_size as i64, d.created_at],
        )
        .map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(())
    })
    .await?;
    log::info!("Registered edge device '{}' (cache_size={})", device.device_name, device.cache_size);
    Ok(device)
}

/// Select top N nodes by attention_score for caching on an edge device.
/// Updates the edge_devices record with the selected node IDs.
pub async fn compute_edge_cache(
    db: &Arc<BrainDb>,
    device_id: String,
    cache_size: u64,
) -> Result<Vec<String>, BrainError> {
    init_edge_cache(db).await?;

    // Select top nodes by attention_score from the attention_focus table
    let limit = cache_size as i64;
    let node_ids: Vec<String> = db
        .with_conn(move |conn| {
            // Try attention_focus first; fall back to quality_score if
            // the attention table is empty (e.g. first run)
            let mut stmt = conn
                .prepare(
                    "SELECT af.node_id FROM attention_focus af
                     JOIN nodes n ON n.id = af.node_id
                     ORDER BY af.attention_score DESC
                     LIMIT ?1",
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt
                .query_map(params![limit], |row| row.get::<_, String>(0))
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let mut ids: Vec<String> = Vec::new();
            for r in rows {
                if let Ok(id) = r {
                    ids.push(id);
                }
            }

            // Fallback: if attention_focus is empty, use quality_score
            if ids.is_empty() {
                let mut stmt2 = conn
                    .prepare(
                        "SELECT id FROM nodes ORDER BY quality_score DESC LIMIT ?1",
                    )
                    .map_err(|e| BrainError::Database(e.to_string()))?;
                let rows2 = stmt2
                    .query_map(params![limit], |row| row.get::<_, String>(0))
                    .map_err(|e| BrainError::Database(e.to_string()))?;
                for r in rows2 {
                    if let Ok(id) = r {
                        ids.push(id);
                    }
                }
            }

            Ok(ids)
        })
        .await?;

    // Update the edge device record
    let ids_json =
        serde_json::to_string(&node_ids).unwrap_or_else(|_| "[]".to_string());
    let did = device_id.clone();
    let now = chrono::Utc::now().to_rfc3339();
    db.with_conn(move |conn| {
        conn.execute(
            "UPDATE edge_devices SET cached_node_ids = ?1, last_synced = ?2 WHERE device_id = ?3",
            params![ids_json, now, did],
        )
        .map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(())
    })
    .await?;

    log::info!(
        "Computed edge cache for device '{}': {} nodes",
        device_id,
        node_ids.len()
    );
    Ok(node_ids)
}

/// Export cached nodes as JSON for the edge device.
pub async fn export_edge_cache(
    db: &Arc<BrainDb>,
    device_id: String,
) -> Result<EdgeCacheExport, BrainError> {
    init_edge_cache(db).await?;

    // Get the device info
    let did = device_id.clone();
    let device: EdgeDevice = db
        .with_conn(move |conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT device_id, device_name, cache_size, cached_node_ids, last_synced, created_at
                     FROM edge_devices WHERE device_id = ?1",
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
            stmt.query_row(params![did], |row| {
                let ids_str: String = row.get(3)?;
                let ids: Vec<String> =
                    serde_json::from_str(&ids_str).unwrap_or_default();
                Ok(EdgeDevice {
                    device_id: row.get(0)?,
                    device_name: row.get(1)?,
                    cache_size: row.get::<_, i64>(2)? as u64,
                    cached_node_ids: ids,
                    last_synced: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })
            .map_err(|e| BrainError::NotFound(format!("edge device: {}", e)))
        })
        .await?;

    if device.cached_node_ids.is_empty() {
        return Ok(EdgeCacheExport {
            device_id: device.device_id,
            device_name: device.device_name,
            nodes: Vec::new(),
            exported_at: chrono::Utc::now().to_rfc3339(),
        });
    }

    // Fetch the actual node data
    let ids = device.cached_node_ids.clone();
    let nodes: Vec<serde_json::Value> = db
        .with_conn(move |conn| {
            let mut result = Vec::new();
            for id in &ids {
                let mut stmt = conn
                    .prepare(
                        "SELECT id, title, content, summary, domain, topic, tags, node_type
                         FROM nodes WHERE id = ?1",
                    )
                    .map_err(|e| BrainError::Database(e.to_string()))?;
                if let Ok(val) = stmt.query_row(params![id], |row| {
                    Ok(serde_json::json!({
                        "id": row.get::<_, String>(0)?,
                        "title": row.get::<_, String>(1)?,
                        "content": row.get::<_, String>(2)?,
                        "summary": row.get::<_, String>(3)?,
                        "domain": row.get::<_, String>(4)?,
                        "topic": row.get::<_, String>(5)?,
                        "tags": row.get::<_, String>(6)?,
                        "node_type": row.get::<_, String>(7)?,
                    }))
                }) {
                    result.push(val);
                }
            }
            Ok(result)
        })
        .await?;

    Ok(EdgeCacheExport {
        device_id: device.device_id,
        device_name: device.device_name,
        nodes,
        exported_at: chrono::Utc::now().to_rfc3339(),
    })
}

/// List all registered edge devices.
pub async fn get_edge_devices(db: &Arc<BrainDb>) -> Result<Vec<EdgeDevice>, BrainError> {
    init_edge_cache(db).await?;

    db.with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT device_id, device_name, cache_size, cached_node_ids, last_synced, created_at
                 FROM edge_devices ORDER BY created_at",
            )
            .map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt
            .query_map([], |row| {
                let ids_str: String = row.get(3)?;
                let ids: Vec<String> =
                    serde_json::from_str(&ids_str).unwrap_or_default();
                Ok(EdgeDevice {
                    device_id: row.get(0)?,
                    device_name: row.get(1)?,
                    cache_size: row.get::<_, i64>(2)? as u64,
                    cached_node_ids: ids,
                    last_synced: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })
            .map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows {
            if let Ok(d) = r {
                result.push(d);
            }
        }
        Ok(result)
    })
    .await
}
