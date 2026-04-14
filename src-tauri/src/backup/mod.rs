use crate::db::BrainDb;
use crate::error::BrainError;
use rusqlite::params;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupInfo {
    pub filename: String,
    pub path: String,
    pub size_bytes: u64,
    pub created_at: String,
    pub node_count: u64,
    pub edge_count: u64,
}

pub async fn create_backup(db: &BrainDb) -> Result<BackupInfo, BrainError> {
    let now = chrono::Utc::now();
    let filename = format!("brain_{}.db", now.format("%Y%m%d_%H%M%S"));
    let backup_dir = db.config.data_dir.join("backups");
    std::fs::create_dir_all(&backup_dir).map_err(|e| BrainError::Io(e))?;
    let backup_path = backup_dir.join(&filename);

    // SQLite backup: just copy the DB file
    let source_path = db.config.sqlite_path();
    std::fs::copy(&source_path, &backup_path).map_err(|e| BrainError::Io(e))?;

    let size_bytes = std::fs::metadata(&backup_path).map(|m| m.len()).unwrap_or(0);

    // Get counts for info
    let (node_count, edge_count) = db.with_conn(|conn| {
        let nodes: u64 = conn.query_row("SELECT COUNT(*) FROM nodes", [], |r| r.get(0))
            .unwrap_or(0);
        let edges: u64 = conn.query_row("SELECT COUNT(*) FROM edges", [], |r| r.get(0))
            .unwrap_or(0);
        Ok((nodes, edges))
    }).await?;

    Ok(BackupInfo {
        filename,
        path: backup_path.to_string_lossy().to_string(),
        size_bytes,
        created_at: now.to_rfc3339(),
        node_count,
        edge_count,
    })
}

pub async fn list_backups(db: &BrainDb) -> Result<Vec<BackupInfo>, BrainError> {
    let backup_dir = db.config.data_dir.join("backups");
    if !backup_dir.exists() { return Ok(vec![]); }

    let mut backups: Vec<BackupInfo> = Vec::new();
    for entry in std::fs::read_dir(&backup_dir).map_err(|e| BrainError::Io(e))? {
        let entry = entry.map_err(|e| BrainError::Io(e))?;
        let path = entry.path();
        // Accept both .json (legacy) and .db (new SQLite backups)
        let is_backup = path.extension().map(|e| e == "json" || e == "db").unwrap_or(false);
        if is_backup {
            let meta = entry.metadata().map_err(|e| BrainError::Io(e))?;
            backups.push(BackupInfo {
                filename: entry.file_name().to_string_lossy().to_string(),
                path: path.to_string_lossy().to_string(),
                size_bytes: meta.len(),
                created_at: meta.modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| chrono::DateTime::from_timestamp(d.as_secs() as i64, 0)
                        .map(|dt| dt.to_rfc3339())
                        .unwrap_or_default())
                    .unwrap_or_default(),
                node_count: 0,
                edge_count: 0,
            });
        }
    }

    backups.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(backups)
}

pub async fn restore_backup(db: &BrainDb, path: &str) -> Result<(u64, u64), BrainError> {
    // Check if it's a legacy JSON backup or a SQLite backup
    if path.ends_with(".json") {
        return restore_json_backup(db, path).await;
    }

    // For .db files, we can't hot-swap the SQLite file while it's open.
    // Instead, read from the backup and insert into the current DB.
    let backup_path = path.to_string();
    let (nodes_restored, edges_restored) = db.with_conn(move |conn| {
        // Attach the backup database
        conn.execute("ATTACH DATABASE ?1 AS backup_db", params![backup_path])
            .map_err(|e| BrainError::Database(format!("Attach failed: {}", e)))?;

        let nodes: u64 = conn.execute(
            "INSERT OR IGNORE INTO nodes SELECT * FROM backup_db.nodes",
            [],
        ).map_err(|e| BrainError::Database(e.to_string()))? as u64;

        let edges: u64 = conn.execute(
            "INSERT OR IGNORE INTO edges SELECT * FROM backup_db.edges",
            [],
        ).map_err(|e| BrainError::Database(e.to_string()))? as u64;

        conn.execute("DETACH DATABASE backup_db", [])
            .map_err(|e| BrainError::Database(e.to_string()))?;

        Ok((nodes, edges))
    }).await?;

    Ok((nodes_restored, edges_restored))
}

async fn restore_json_backup(db: &BrainDb, path: &str) -> Result<(u64, u64), BrainError> {
    #[derive(Debug, Deserialize)]
    struct BackupData {
        nodes: Vec<serde_json::Value>,
        edges: Vec<serde_json::Value>,
    }

    let json = std::fs::read_to_string(path).map_err(|e| BrainError::Io(e))?;
    let data: BackupData = serde_json::from_str(&json).map_err(|e| BrainError::Serialization(e))?;

    let nodes_count = data.nodes.len() as u64;
    let edges_count = data.edges.len() as u64;

    // Import nodes using create_node for proper dedup
    for node in &data.nodes {
        let title = node.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let content = node.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let domain = node.get("domain").and_then(|v| v.as_str()).unwrap_or("general").to_string();
        let topic = node.get("topic").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let tags: Vec<String> = node.get("tags").and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|t| t.as_str().map(String::from)).collect())
            .unwrap_or_default();
        let node_type = node.get("node_type").and_then(|v| v.as_str()).unwrap_or("reference").to_string();
        let source_type = node.get("source_type").and_then(|v| v.as_str()).unwrap_or("manual").to_string();
        let source_url = node.get("source_url").and_then(|v| v.as_str()).map(String::from);

        let input = crate::db::models::CreateNodeInput {
            title, content, domain, topic, tags, node_type, source_type, source_url,
        };
        let _ = db.create_node(input).await;
    }

    Ok((nodes_count, edges_count))
}
