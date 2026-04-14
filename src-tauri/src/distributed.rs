//! Phase Omega Part VII — Distributed Brain Architecture
//!
//! Multi-node brain coordination over the network. Allows the brain to
//! span multiple machines: a primary node, GPU compute nodes, web
//! scrapers, and lightweight edge devices. Nodes register themselves,
//! send heartbeats, and sync knowledge bidirectionally via HTTP.

use crate::db::BrainDb;
use crate::error::BrainError;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::sync::Arc;

// =========================================================================
// Models
// =========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainNode {
    pub id: String,
    pub name: String,
    pub role: String,
    pub endpoint_url: String,
    pub status: String,
    pub last_heartbeat: Option<String>,
    pub capabilities: Vec<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStatus {
    pub node_id: String,
    pub last_sync: String,
    pub nodes_synced: u64,
    pub pending: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterStatus {
    pub nodes: Vec<BrainNode>,
    pub sync_statuses: Vec<SyncStatus>,
    pub total_nodes: usize,
    pub online_count: usize,
}

// =========================================================================
// Schema initialisation
// =========================================================================

pub async fn init_distributed(db: &Arc<BrainDb>) -> Result<(), BrainError> {
    db.with_conn(|conn| {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS brain_nodes (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                role TEXT NOT NULL DEFAULT 'primary',
                endpoint_url TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'offline',
                last_heartbeat TEXT,
                capabilities TEXT NOT NULL DEFAULT '[]',
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS sync_log (
                id TEXT PRIMARY KEY,
                node_id TEXT NOT NULL,
                direction TEXT NOT NULL,
                nodes_synced INTEGER NOT NULL DEFAULT 0,
                status TEXT NOT NULL DEFAULT 'ok',
                created_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_brain_nodes_status ON brain_nodes(status);
            CREATE INDEX IF NOT EXISTS idx_sync_log_node ON sync_log(node_id);
            CREATE INDEX IF NOT EXISTS idx_sync_log_created ON sync_log(created_at);
            ",
        )
        .map_err(|e| BrainError::Database(format!("distributed schema init: {}", e)))?;
        Ok(())
    })
    .await
}

// =========================================================================
// Core operations
// =========================================================================

/// Register a brain node in the distributed architecture.
pub async fn register_node(
    db: &Arc<BrainDb>,
    name: String,
    role: String,
    endpoint_url: String,
) -> Result<BrainNode, BrainError> {
    let id = format!("bnode:{}", uuid::Uuid::now_v7());
    let now = chrono::Utc::now().to_rfc3339();
    let node = BrainNode {
        id: id.clone(),
        name,
        role,
        endpoint_url,
        status: "online".to_string(),
        last_heartbeat: Some(now.clone()),
        capabilities: Vec::new(),
        created_at: now.clone(),
    };
    let n = node.clone();
    db.with_conn(move |conn| {
        let caps_json =
            serde_json::to_string(&n.capabilities).unwrap_or_else(|_| "[]".to_string());
        conn.execute(
            "INSERT INTO brain_nodes (id, name, role, endpoint_url, status, last_heartbeat, capabilities, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                n.id,
                n.name,
                n.role,
                n.endpoint_url,
                n.status,
                n.last_heartbeat,
                caps_json,
                n.created_at,
            ],
        )
        .map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(())
    })
    .await?;
    log::info!("Registered brain node '{}' (role={})", node.name, node.role);
    Ok(node)
}

/// Update a node's heartbeat timestamp.
pub async fn heartbeat(db: &Arc<BrainDb>, node_id: String) -> Result<(), BrainError> {
    let now = chrono::Utc::now().to_rfc3339();
    db.with_conn(move |conn| {
        conn.execute(
            "UPDATE brain_nodes SET last_heartbeat = ?1, status = 'online' WHERE id = ?2",
            params![now, node_id],
        )
        .map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(())
    })
    .await
}

/// Check all registered nodes and mark offline if no heartbeat in 5 minutes.
pub async fn check_nodes_health(db: &Arc<BrainDb>) -> Result<u64, BrainError> {
    let cutoff = (chrono::Utc::now() - chrono::Duration::minutes(5)).to_rfc3339();
    db.with_conn(move |conn| {
        let changed = conn
            .execute(
                "UPDATE brain_nodes SET status = 'offline'
                 WHERE status = 'online'
                   AND (last_heartbeat IS NULL OR last_heartbeat < ?1)",
                params![cutoff],
            )
            .map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(changed as u64)
    })
    .await
}

/// Push new/updated nodes to a remote brain node via HTTP POST.
pub async fn sync_to_node(db: &Arc<BrainDb>, node_id: String) -> Result<SyncStatus, BrainError> {
    // 1. Look up the target node's endpoint
    let nid = node_id.clone();
    let target = db
        .with_conn(move |conn| {
            let mut stmt = conn
                .prepare("SELECT endpoint_url FROM brain_nodes WHERE id = ?1")
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let url: String = stmt
                .query_row(params![nid], |row| row.get(0))
                .map_err(|e| BrainError::NotFound(format!("node {}: {}", nid, e)))?;
            Ok(url)
        })
        .await?;

    // 2. Get last sync timestamp for this node
    let nid2 = node_id.clone();
    let last_sync = db
        .with_conn(move |conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT created_at FROM sync_log
                     WHERE node_id = ?1 AND direction = 'push'
                     ORDER BY created_at DESC LIMIT 1",
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let ts: Option<String> = stmt
                .query_row(params![nid2], |row| row.get(0))
                .ok();
            Ok(ts.unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string()))
        })
        .await?;

    // 3. Gather nodes updated since last sync
    let ls = last_sync.clone();
    let nodes_to_push: Vec<serde_json::Value> = db
        .with_conn(move |conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT id, title, content, summary, domain, topic, tags, node_type, created_at, updated_at
                     FROM nodes WHERE updated_at > ?1 LIMIT 500",
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt
                .query_map(params![ls], |row| {
                    Ok(serde_json::json!({
                        "id": row.get::<_, String>(0)?,
                        "title": row.get::<_, String>(1)?,
                        "content": row.get::<_, String>(2)?,
                        "summary": row.get::<_, String>(3)?,
                        "domain": row.get::<_, String>(4)?,
                        "topic": row.get::<_, String>(5)?,
                        "tags": row.get::<_, String>(6)?,
                        "node_type": row.get::<_, String>(7)?,
                        "created_at": row.get::<_, String>(8)?,
                        "updated_at": row.get::<_, String>(9)?,
                    }))
                })
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let mut result = Vec::new();
            for r in rows {
                if let Ok(v) = r {
                    result.push(v);
                }
            }
            Ok(result)
        })
        .await?;

    let pushed = nodes_to_push.len() as u64;

    // 4. POST to the remote node
    if pushed > 0 {
        let client = reqwest::Client::new();
        let url = format!("{}/brain/sync-receive", target.trim_end_matches('/'));
        let _ = client
            .post(&url)
            .json(&serde_json::json!({ "nodes": nodes_to_push }))
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await
            .map_err(|e| {
                log::warn!("sync_to_node push failed for {}: {}", node_id, e);
            });
    }

    // 5. Log the sync
    let now = chrono::Utc::now().to_rfc3339();
    let log_id = format!("sync:{}", uuid::Uuid::now_v7());
    let nid3 = node_id.clone();
    let now2 = now.clone();
    db.with_conn(move |conn| {
        conn.execute(
            "INSERT INTO sync_log (id, node_id, direction, nodes_synced, status, created_at)
             VALUES (?1, ?2, 'push', ?3, 'ok', ?4)",
            params![log_id, nid3, pushed, now2],
        )
        .map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(())
    })
    .await?;

    log::info!("Synced {} nodes to node {}", pushed, node_id);
    Ok(SyncStatus {
        node_id,
        last_sync: now,
        nodes_synced: pushed,
        pending: 0,
    })
}

/// Pull new nodes from a remote brain node.
pub async fn sync_from_node(
    db: &Arc<BrainDb>,
    node_id: String,
) -> Result<SyncStatus, BrainError> {
    // 1. Look up endpoint
    let nid = node_id.clone();
    let target = db
        .with_conn(move |conn| {
            let mut stmt = conn
                .prepare("SELECT endpoint_url FROM brain_nodes WHERE id = ?1")
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let url: String = stmt
                .query_row(params![nid], |row| row.get(0))
                .map_err(|e| BrainError::NotFound(format!("node: {}", e)))?;
            Ok(url)
        })
        .await?;

    // 2. GET from remote
    let client = reqwest::Client::new();
    let nid2 = node_id.clone();
    let last_sync = db
        .with_conn(move |conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT created_at FROM sync_log
                     WHERE node_id = ?1 AND direction = 'pull'
                     ORDER BY created_at DESC LIMIT 1",
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let ts: Option<String> = stmt.query_row(params![nid2], |row| row.get(0)).ok();
            Ok(ts.unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string()))
        })
        .await?;

    let url = format!(
        "{}/brain/sync-export?since={}",
        target.trim_end_matches('/'),
        last_sync
    );
    let resp = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| BrainError::Http(e))?;

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| BrainError::Http(e))?;

    let remote_nodes = body
        .get("nodes")
        .and_then(|n| n.as_array())
        .cloned()
        .unwrap_or_default();

    let pulled = remote_nodes.len() as u64;

    // 3. Upsert received nodes
    if pulled > 0 {
        let nodes_data = remote_nodes.clone();
        db.with_conn(move |conn| {
            for node_val in &nodes_data {
                let id = node_val.get("id").and_then(|v| v.as_str()).unwrap_or("");
                let title = node_val.get("title").and_then(|v| v.as_str()).unwrap_or("");
                let content = node_val
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if id.is_empty() || content.is_empty() {
                    continue;
                }
                let hash = format!("{:x}", sha2::Digest::finalize(sha2::Sha256::new().chain_update(content)));
                let domain = node_val
                    .get("domain")
                    .and_then(|v| v.as_str())
                    .unwrap_or("general");
                let topic = node_val
                    .get("topic")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let tags = node_val
                    .get("tags")
                    .and_then(|v| v.as_str())
                    .unwrap_or("[]");
                let node_type = node_val
                    .get("node_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("reference");
                let summary = node_val
                    .get("summary")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let now = chrono::Utc::now().to_rfc3339();
                let created = node_val
                    .get("created_at")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&now);
                // INSERT OR IGNORE — don't overwrite existing nodes
                let _ = conn.execute(
                    "INSERT OR IGNORE INTO nodes (id, title, content, summary, content_hash, domain, topic, tags, node_type, source_type, created_at, updated_at, accessed_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'sync', ?10, ?10, ?10)",
                    params![id, title, content, summary, hash, domain, topic, tags, node_type, created],
                );
            }
            Ok(())
        })
        .await?;
    }

    // 4. Log
    let now = chrono::Utc::now().to_rfc3339();
    let log_id = format!("sync:{}", uuid::Uuid::now_v7());
    let nid3 = node_id.clone();
    let now2 = now.clone();
    db.with_conn(move |conn| {
        conn.execute(
            "INSERT INTO sync_log (id, node_id, direction, nodes_synced, status, created_at)
             VALUES (?1, ?2, 'pull', ?3, 'ok', ?4)",
            params![log_id, nid3, pulled, now2],
        )
        .map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(())
    })
    .await?;

    log::info!("Pulled {} nodes from node {}", pulled, node_id);
    Ok(SyncStatus {
        node_id,
        last_sync: now,
        nodes_synced: pulled,
        pending: 0,
    })
}

/// Return all nodes with their status and sync info.
pub async fn get_cluster_status(db: &Arc<BrainDb>) -> Result<ClusterStatus, BrainError> {
    init_distributed(db).await?;

    let nodes: Vec<BrainNode> = db
        .with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT id, name, role, endpoint_url, status, last_heartbeat, capabilities, created_at
                     FROM brain_nodes ORDER BY created_at",
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt
                .query_map([], |row| {
                    let caps_str: String = row.get(6)?;
                    let caps: Vec<String> =
                        serde_json::from_str(&caps_str).unwrap_or_default();
                    Ok(BrainNode {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        role: row.get(2)?,
                        endpoint_url: row.get(3)?,
                        status: row.get(4)?,
                        last_heartbeat: row.get(5)?,
                        capabilities: caps,
                        created_at: row.get(7)?,
                    })
                })
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let mut result = Vec::new();
            for r in rows {
                if let Ok(n) = r {
                    result.push(n);
                }
            }
            Ok(result)
        })
        .await?;

    let sync_statuses: Vec<SyncStatus> = db
        .with_conn(|conn| {
            // Latest sync per node
            let mut stmt = conn
                .prepare(
                    "SELECT node_id, MAX(created_at) as last_sync,
                            SUM(nodes_synced) as total_synced
                     FROM sync_log GROUP BY node_id",
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt
                .query_map([], |row| {
                    Ok(SyncStatus {
                        node_id: row.get(0)?,
                        last_sync: row.get(1)?,
                        nodes_synced: row.get::<_, i64>(2).unwrap_or(0) as u64,
                        pending: 0,
                    })
                })
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let mut result = Vec::new();
            for r in rows {
                if let Ok(s) = r {
                    result.push(s);
                }
            }
            Ok(result)
        })
        .await?;

    let online_count = nodes.iter().filter(|n| n.status == "online").count();
    let total_nodes = nodes.len();

    Ok(ClusterStatus {
        nodes,
        sync_statuses,
        total_nodes,
        online_count,
    })
}

/// Check if a query should be routed to a specialized node based on
/// node capabilities. Returns the endpoint URL if routing is appropriate,
/// or None if the query should be handled locally.
pub async fn route_query(
    db: &Arc<BrainDb>,
    query: String,
) -> Result<Option<String>, BrainError> {
    init_distributed(db).await?;

    let q = query.to_lowercase();
    let nodes: Vec<BrainNode> = db
        .with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT id, name, role, endpoint_url, status, last_heartbeat, capabilities, created_at
                     FROM brain_nodes WHERE status = 'online' AND role != 'primary'",
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt
                .query_map([], |row| {
                    let caps_str: String = row.get(6)?;
                    let caps: Vec<String> =
                        serde_json::from_str(&caps_str).unwrap_or_default();
                    Ok(BrainNode {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        role: row.get(2)?,
                        endpoint_url: row.get(3)?,
                        status: row.get(4)?,
                        last_heartbeat: row.get(5)?,
                        capabilities: caps,
                        created_at: row.get(7)?,
                    })
                })
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let mut result = Vec::new();
            for r in rows {
                if let Ok(n) = r {
                    result.push(n);
                }
            }
            Ok(result)
        })
        .await?;

    // Simple keyword matching against node capabilities
    for node in &nodes {
        for cap in &node.capabilities {
            if q.contains(&cap.to_lowercase()) {
                log::info!(
                    "Routing query to node '{}' (capability: {})",
                    node.name,
                    cap
                );
                return Ok(Some(node.endpoint_url.clone()));
            }
        }
    }

    Ok(None)
}
