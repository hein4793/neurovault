//! Phase Omega Part VI — The Collective: Multi-Brain Federation
//!
//! Enables multiple NeuroVault instances to share knowledge, sync
//! decisions, and request expertise from each other. Each brain can
//! register federated peers, share selected nodes with them via HTTP,
//! and receive shared knowledge back. Privacy levels control what gets
//! shared: "public" (anyone), "team" (registered peers), or "private"
//! (never shared).
//!
//! ## Architecture
//!
//! - `FederatedBrain` — a registered peer brain with endpoint + keys
//! - `FederationMessage` — an outbound/inbound message between brains
//! - `SharedKnowledge` — imported knowledge from a peer brain
//! - `register_brain()` — add a new peer
//! - `share_knowledge()` — push selected nodes to a peer
//! - `receive_knowledge()` — accept inbound shared knowledge
//! - `sync_with_brain()` — two-way sync of shareable knowledge
//! - `get_federation_status()` — list all peers and sync status

use crate::db::BrainDb;
use crate::error::BrainError;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// =========================================================================
// DATA STRUCTURES
// =========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederatedBrain {
    pub id: String,
    pub name: String,
    pub endpoint_url: String,
    pub public_key: String,
    pub privacy_level: String, // "public", "team", "private"
    pub last_synced: Option<String>,
    pub enabled: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationMessage {
    pub id: String,
    pub from_brain_id: String,
    pub to_brain_id: String,
    pub message_type: String, // "share_knowledge", "request_expertise", "sync_decision"
    pub payload: String,      // JSON
    pub privacy_level: String,
    pub status: String, // "pending", "sent", "received", "failed"
    pub created_at: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedKnowledge {
    pub id: String,
    pub source_brain_id: String,
    pub local_node_id: Option<String>,
    pub title: String,
    pub domain: String,
    pub quality_score: f32,
    pub imported_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationStatus {
    pub brain_count: usize,
    pub brains: Vec<FederatedBrain>,
    pub pending_messages: usize,
    pub total_shared_knowledge: usize,
}

// =========================================================================
// SCHEMA INIT
// =========================================================================

/// Create federation tables (idempotent).
pub async fn init_federation(db: &Arc<BrainDb>) -> Result<(), BrainError> {
    db.with_conn(|conn| {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS federated_brains (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                endpoint_url TEXT NOT NULL,
                public_key TEXT NOT NULL DEFAULT '',
                privacy_level TEXT NOT NULL DEFAULT 'team',
                last_synced TEXT,
                enabled INTEGER NOT NULL DEFAULT 1,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS federation_messages (
                id TEXT PRIMARY KEY,
                from_brain_id TEXT NOT NULL,
                to_brain_id TEXT NOT NULL,
                message_type TEXT NOT NULL,
                payload TEXT NOT NULL DEFAULT '{}',
                privacy_level TEXT NOT NULL DEFAULT 'team',
                status TEXT NOT NULL DEFAULT 'pending',
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS shared_knowledge (
                id TEXT PRIMARY KEY,
                source_brain_id TEXT NOT NULL,
                local_node_id TEXT,
                title TEXT NOT NULL,
                domain TEXT NOT NULL DEFAULT 'general',
                quality_score REAL NOT NULL DEFAULT 0.5,
                imported_at TEXT NOT NULL
            );"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(())
    }).await
}

// =========================================================================
// CORE FUNCTIONS
// =========================================================================

/// Register a new federated brain peer.
pub async fn register_brain(
    db: &Arc<BrainDb>,
    name: String,
    endpoint_url: String,
) -> Result<FederatedBrain, BrainError> {
    init_federation(db).await?;

    let id = format!("fed_brain:{}", uuid::Uuid::now_v7());
    let now = chrono::Utc::now().to_rfc3339();

    let brain = FederatedBrain {
        id: id.clone(),
        name: name.clone(),
        endpoint_url: endpoint_url.clone(),
        public_key: String::new(),
        privacy_level: "team".to_string(),
        last_synced: None,
        enabled: true,
        created_at: now.clone(),
    };

    let b = brain.clone();
    db.with_conn(move |conn| {
        conn.execute(
            "INSERT INTO federated_brains (id, name, endpoint_url, public_key, privacy_level, last_synced, enabled, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![b.id, b.name, b.endpoint_url, b.public_key, b.privacy_level, b.last_synced, b.enabled as i32, b.created_at],
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(())
    }).await?;

    log::info!("Registered federated brain '{}' at {}", name, endpoint_url);
    Ok(brain)
}

/// Share selected knowledge nodes with a federated brain via HTTP POST.
pub async fn share_knowledge(
    db: &Arc<BrainDb>,
    brain_id: &str,
    node_ids: Vec<String>,
) -> Result<String, BrainError> {
    init_federation(db).await?;

    // Look up the target brain
    let bid = brain_id.to_string();
    let target: FederatedBrain = db.with_conn(move |conn| {
        conn.query_row(
            "SELECT id, name, endpoint_url, public_key, privacy_level, last_synced, enabled, created_at
             FROM federated_brains WHERE id = ?1 AND enabled = 1",
            params![bid],
            |row| {
                Ok(FederatedBrain {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    endpoint_url: row.get(2)?,
                    public_key: row.get(3)?,
                    privacy_level: row.get(4)?,
                    last_synced: row.get(5)?,
                    enabled: row.get::<_, i32>(6)? != 0,
                    created_at: row.get(7)?,
                })
            },
        ).map_err(|e| BrainError::NotFound(format!("Federated brain not found: {}", e)))
    }).await?;

    // Fetch node data for the selected nodes
    let ids = node_ids.clone();
    let nodes_data: Vec<serde_json::Value> = db.with_conn(move |conn| {
        let mut result = Vec::new();
        for nid in &ids {
            let row = conn.query_row(
                "SELECT id, title, content, summary, domain, topic, tags, node_type, quality_score
                 FROM nodes WHERE id = ?1",
                params![nid],
                |row| {
                    Ok(serde_json::json!({
                        "id": row.get::<_, String>(0)?,
                        "title": row.get::<_, String>(1)?,
                        "content": row.get::<_, String>(2)?,
                        "summary": row.get::<_, String>(3)?,
                        "domain": row.get::<_, String>(4)?,
                        "topic": row.get::<_, String>(5)?,
                        "tags": row.get::<_, String>(6)?,
                        "node_type": row.get::<_, String>(7)?,
                        "quality_score": row.get::<_, f64>(8)?,
                    }))
                },
            );
            if let Ok(n) = row {
                result.push(n);
            }
        }
        Ok(result)
    }).await?;

    if nodes_data.is_empty() {
        return Ok("No nodes found to share".to_string());
    }

    // Create a federation message
    let msg_id = format!("fed_msg:{}", uuid::Uuid::now_v7());
    let now = chrono::Utc::now().to_rfc3339();
    let payload = serde_json::to_string(&nodes_data)
        .map_err(|e| BrainError::Internal(e.to_string()))?;

    let msg = FederationMessage {
        id: msg_id.clone(),
        from_brain_id: "self".to_string(),
        to_brain_id: target.id.clone(),
        message_type: "share_knowledge".to_string(),
        payload: payload.clone(),
        privacy_level: target.privacy_level.clone(),
        status: "pending".to_string(),
        created_at: now.clone(),
    };

    // Store the message locally
    let m = msg.clone();
    db.with_conn(move |conn| {
        conn.execute(
            "INSERT INTO federation_messages (id, from_brain_id, to_brain_id, message_type, payload, privacy_level, status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![m.id, m.from_brain_id, m.to_brain_id, m.message_type, m.payload, m.privacy_level, m.status, m.created_at],
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(())
    }).await?;

    // Send HTTP POST to the target brain's endpoint
    let endpoint = format!("{}/federation/receive", target.endpoint_url.trim_end_matches('/'));
    let client = reqwest::Client::new();
    let send_result = client
        .post(&endpoint)
        .json(&serde_json::json!({
            "message_id": msg_id,
            "from_brain": "self",
            "message_type": "share_knowledge",
            "nodes": nodes_data,
        }))
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await;

    // Update message status
    let status = match &send_result {
        Ok(resp) if resp.status().is_success() => "sent",
        Ok(_) => "failed",
        Err(_) => "failed",
    };

    let final_status = status.to_string();
    let mid = msg_id.clone();
    db.with_conn(move |conn| {
        conn.execute(
            "UPDATE federation_messages SET status = ?1 WHERE id = ?2",
            params![final_status, mid],
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(())
    }).await?;

    let summary = format!(
        "Shared {} nodes with '{}' — status: {}",
        nodes_data.len(),
        target.name,
        status
    );
    log::info!("{}", summary);
    Ok(summary)
}

/// Receive shared knowledge from another brain and create local nodes.
pub async fn receive_knowledge(
    db: &Arc<BrainDb>,
    from_brain_id: String,
    nodes: Vec<serde_json::Value>,
) -> Result<String, BrainError> {
    init_federation(db).await?;

    let now = chrono::Utc::now().to_rfc3339();
    let mut imported = 0u32;
    let mut skipped = 0u32;

    for node in &nodes {
        let title = node.get("title").and_then(|v| v.as_str()).unwrap_or("Untitled").to_string();
        let content = node.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let summary = node.get("summary").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let domain = node.get("domain").and_then(|v| v.as_str()).unwrap_or("general").to_string();
        let topic = node.get("topic").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let tags = node.get("tags").and_then(|v| v.as_str()).unwrap_or("[]").to_string();
        let node_type = node.get("node_type").and_then(|v| v.as_str()).unwrap_or("reference").to_string();
        let quality = node.get("quality_score").and_then(|v| v.as_f64()).unwrap_or(0.5) as f32;

        if content.is_empty() {
            skipped += 1;
            continue;
        }

        // Compute content hash for dedup
        use sha2::{Digest, Sha256};
        let content_hash = format!("{:x}", Sha256::digest(content.as_bytes()));

        let node_id = format!("node:{}", uuid::Uuid::now_v7());
        let shared_id = format!("shared:{}", uuid::Uuid::now_v7());
        let ts = now.clone();
        let fbid = from_brain_id.clone();
        let nid = node_id.clone();
        let t = title.clone();
        let d = domain.clone();
        let q = quality;

        let result = db.with_conn(move |conn| {
            // Check if content already exists
            let exists: bool = conn.query_row(
                "SELECT COUNT(*) FROM nodes WHERE content_hash = ?1",
                params![content_hash],
                |row| row.get::<_, u64>(0),
            ).unwrap_or(0) > 0;

            if exists {
                return Ok(false);
            }

            // Insert the node
            conn.execute(
                "INSERT INTO nodes (id, title, content, summary, content_hash, domain, topic, tags,
                                    node_type, source_type, quality_score, visual_size,
                                    decay_score, access_count, synthesized_by_brain,
                                    created_at, updated_at, accessed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'federation', ?10, 3.0, 1.0, 0, 0, ?11, ?11, ?11)",
                params![nid, t, content, summary, content_hash, d, topic, tags, node_type, q as f64, ts],
            ).map_err(|e| BrainError::Database(e.to_string()))?;

            // Track in shared_knowledge
            conn.execute(
                "INSERT INTO shared_knowledge (id, source_brain_id, local_node_id, title, domain, quality_score, imported_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![shared_id, fbid, nid, t, d, q, ts],
            ).map_err(|e| BrainError::Database(e.to_string()))?;

            Ok(true)
        }).await?;

        if result {
            imported += 1;
        } else {
            skipped += 1;
        }
    }

    let summary = format!(
        "Received knowledge from '{}': {} imported, {} skipped (duplicates)",
        from_brain_id, imported, skipped
    );
    log::info!("{}", summary);
    Ok(summary)
}

/// Two-way sync of shareable knowledge with a federated brain.
pub async fn sync_with_brain(
    db: &Arc<BrainDb>,
    brain_id: &str,
) -> Result<String, BrainError> {
    init_federation(db).await?;

    // Look up the target brain
    let bid = brain_id.to_string();
    let target: FederatedBrain = db.with_conn(move |conn| {
        conn.query_row(
            "SELECT id, name, endpoint_url, public_key, privacy_level, last_synced, enabled, created_at
             FROM federated_brains WHERE id = ?1 AND enabled = 1",
            params![bid],
            |row| {
                Ok(FederatedBrain {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    endpoint_url: row.get(2)?,
                    public_key: row.get(3)?,
                    privacy_level: row.get(4)?,
                    last_synced: row.get(5)?,
                    enabled: row.get::<_, i32>(6)? != 0,
                    created_at: row.get(7)?,
                })
            },
        ).map_err(|e| BrainError::NotFound(format!("Federated brain not found: {}", e)))
    }).await?;

    // Get our shareable nodes (public + team privacy)
    let since = target.last_synced.clone().unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string());
    let our_nodes: Vec<serde_json::Value> = db.with_conn(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, title, summary, domain, quality_score FROM nodes
             WHERE updated_at > ?1
             AND quality_score > 0.4
             ORDER BY quality_score DESC
             LIMIT 100"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map(params![since], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "title": row.get::<_, String>(1)?,
                "summary": row.get::<_, String>(2)?,
                "domain": row.get::<_, String>(3)?,
                "quality_score": row.get::<_, f64>(4)?,
            }))
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows {
            if let Ok(v) = r { result.push(v); }
        }
        Ok(result)
    }).await?;

    // Send sync request to target brain
    let endpoint = format!("{}/federation/sync", target.endpoint_url.trim_end_matches('/'));
    let client = reqwest::Client::new();
    let resp = client
        .post(&endpoint)
        .json(&serde_json::json!({
            "from_brain": "self",
            "nodes": our_nodes,
            "since": target.last_synced,
        }))
        .timeout(std::time::Duration::from_secs(60))
        .send()
        .await;

    let mut received_count = 0usize;
    if let Ok(resp) = resp {
        if resp.status().is_success() {
            if let Ok(body) = resp.json::<serde_json::Value>().await {
                if let Some(nodes) = body.get("nodes").and_then(|v| v.as_array()) {
                    received_count = nodes.len();
                    let _ = receive_knowledge(
                        db,
                        target.id.clone(),
                        nodes.clone(),
                    ).await;
                }
            }
        }
    }

    // Update last_synced timestamp
    let now = chrono::Utc::now().to_rfc3339();
    let tid = target.id.clone();
    db.with_conn(move |conn| {
        conn.execute(
            "UPDATE federated_brains SET last_synced = ?1 WHERE id = ?2",
            params![now, tid],
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(())
    }).await?;

    let summary = format!(
        "Synced with '{}': sent {} nodes, received {} nodes",
        target.name, our_nodes.len(), received_count
    );
    log::info!("{}", summary);
    Ok(summary)
}

/// Get the current federation status — all brains, message counts, shared knowledge.
pub async fn get_federation_status(db: &Arc<BrainDb>) -> Result<FederationStatus, BrainError> {
    init_federation(db).await?;

    db.with_conn(|conn| {
        // List all federated brains
        let mut stmt = conn.prepare(
            "SELECT id, name, endpoint_url, public_key, privacy_level, last_synced, enabled, created_at
             FROM federated_brains ORDER BY created_at DESC"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok(FederatedBrain {
                id: row.get(0)?,
                name: row.get(1)?,
                endpoint_url: row.get(2)?,
                public_key: row.get(3)?,
                privacy_level: row.get(4)?,
                last_synced: row.get(5)?,
                enabled: row.get::<_, i32>(6)? != 0,
                created_at: row.get(7)?,
            })
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut brains = Vec::new();
        for r in rows {
            if let Ok(b) = r { brains.push(b); }
        }

        // Count pending messages
        let pending_messages: usize = conn.query_row(
            "SELECT COUNT(*) FROM federation_messages WHERE status = 'pending'",
            [],
            |row| row.get(0),
        ).unwrap_or(0);

        // Count shared knowledge
        let total_shared_knowledge: usize = conn.query_row(
            "SELECT COUNT(*) FROM shared_knowledge",
            [],
            |row| row.get(0),
        ).unwrap_or(0);

        let brain_count = brains.len();

        Ok(FederationStatus {
            brain_count,
            brains,
            pending_messages,
            total_shared_knowledge,
        })
    }).await
}

// =========================================================================
// CIRCUIT — federation_sync
// =========================================================================

/// Circuit entry point: sync with all enabled federated brains.
pub async fn circuit_federation_sync(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    init_federation(db).await?;

    // Get all enabled brains
    let brains: Vec<String> = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id FROM federated_brains WHERE enabled = 1"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows {
            if let Ok(id) = r { result.push(id); }
        }
        Ok(result)
    }).await?;

    if brains.is_empty() {
        return Ok("No federated brains configured".to_string());
    }

    let mut synced = 0u32;
    let mut failed = 0u32;
    for brain_id in &brains {
        match sync_with_brain(db, brain_id).await {
            Ok(_) => synced += 1,
            Err(e) => {
                log::warn!("Federation sync failed for {}: {}", brain_id, e);
                failed += 1;
            }
        }
    }

    Ok(format!(
        "Federation sync: {} brains synced, {} failed",
        synced, failed
    ))
}
