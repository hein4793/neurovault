//! Cold Storage — export cold-tier nodes to versioned JSONL archives.
//! This module never deletes anything from SQLite. Cold storage is purely
//! export + audit trail. Deletion requires explicit user confirmation via
//! `purge_archive`.

use crate::db::BrainDb;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

pub async fn run_cold_storage_loop(db: Arc<BrainDb>) {
    tokio::time::sleep(Duration::from_secs(1800)).await;
    log::info!("Cold storage loop started");
    loop {
        match run_one_pass(&db).await {
            Ok(stats) => log::info!("Cold storage pass: archived {} nodes ({} bytes)", stats.archived, stats.bytes_written),
            Err(e) => log::warn!("Cold storage pass failed: {}", e),
        }
        tokio::time::sleep(Duration::from_secs(604_800)).await;
    }
}

#[derive(Debug, Default)]
pub struct ColdStats { pub archived: u64, pub bytes_written: u64, pub archive_path: Option<PathBuf> }

pub async fn run_one_pass(db: &BrainDb) -> Result<ColdStats, String> {
    let mut stats = ColdStats::default();
    let cutoff = (chrono::Utc::now() - chrono::Duration::days(30)).to_rfc3339();

    let cold_nodes: Vec<serde_json::Value> = db.with_conn(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, title, content, summary, content_hash, domain, topic, tags, \
             node_type, source_type, source_url, source_file, quality_score, decay_score, \
             created_at, updated_at, accessed_at, access_count \
             FROM nodes WHERE memory_tier = 'cold' AND updated_at < ?1 LIMIT 5000"
        ).map_err(|e| crate::error::BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map(params![cutoff], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "title": row.get::<_, String>(1)?,
                "content": row.get::<_, String>(2)?,
                "summary": row.get::<_, String>(3)?,
                "content_hash": row.get::<_, String>(4)?,
                "domain": row.get::<_, String>(5)?,
                "topic": row.get::<_, String>(6)?,
                "tags": row.get::<_, String>(7)?,
                "node_type": row.get::<_, String>(8)?,
                "source_type": row.get::<_, String>(9)?,
                "source_url": row.get::<_, Option<String>>(10)?,
                "source_file": row.get::<_, Option<String>>(11)?,
                "quality_score": row.get::<_, f64>(12)?,
                "decay_score": row.get::<_, f64>(13)?,
                "created_at": row.get::<_, String>(14)?,
                "updated_at": row.get::<_, String>(15)?,
                "accessed_at": row.get::<_, String>(16)?,
                "access_count": row.get::<_, u64>(17)?,
            }))
        }).map_err(|e| crate::error::BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(v) = r { result.push(v); } }
        Ok(result)
    }).await.map_err(|e| e.to_string())?;

    if cold_nodes.is_empty() { return Ok(stats); }

    let cold_dir = db.config.data_dir.join("cold");
    std::fs::create_dir_all(&cold_dir).map_err(|e| format!("mkdir: {}", e))?;
    let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string();
    let archive_path = cold_dir.join(format!("archive-{}.jsonl", timestamp));

    use std::io::Write;
    let file = std::fs::File::create(&archive_path).map_err(|e| format!("create: {}", e))?;
    let mut writer = std::io::BufWriter::new(file);
    let mut bytes_written = 0u64;

    for n in &cold_nodes {
        let mut entry = n.clone();
        entry.as_object_mut().map(|m| {
            m.insert("archived_at".into(), serde_json::json!(chrono::Utc::now().to_rfc3339()));
            m.insert("memory_tier".into(), serde_json::json!("cold"));
        });
        let line = serde_json::to_string(&entry).map_err(|e| e.to_string())?;
        writeln!(writer, "{}", line).map_err(|e| e.to_string())?;
        bytes_written += line.len() as u64 + 1;
        stats.archived += 1;
    }
    writer.flush().map_err(|e| e.to_string())?;

    stats.bytes_written = bytes_written;
    stats.archive_path = Some(archive_path.clone());

    // Log the archive
    let now = chrono::Utc::now().to_rfc3339();
    let path_str = archive_path.to_string_lossy().to_string();
    let count = stats.archived;
    let _bytes = stats.bytes_written;
    let _ = db.with_conn(move |conn| {
        let id = format!("cold_archive_log:{}", uuid::Uuid::now_v7());
        conn.execute(
            "INSERT INTO cold_archive_log (id, archive_path, node_count, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![id, path_str, count, now],
        ).map_err(|e| crate::error::BrainError::Database(e.to_string()))
    }).await;

    Ok(stats)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColdArchiveEntry { pub path: String, pub archived_at: String, pub node_count: u64, pub bytes_written: u64, pub status: String }

pub async fn list_archives(db: &BrainDb) -> Result<Vec<ColdArchiveEntry>, String> {
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT archive_path, created_at, node_count FROM cold_archive_log ORDER BY created_at DESC LIMIT 200"
        ).map_err(|e| crate::error::BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok(ColdArchiveEntry {
                path: row.get(0)?,
                archived_at: row.get(1)?,
                node_count: row.get(2)?,
                bytes_written: 0,
                status: "archived".to_string(),
            })
        }).map_err(|e| crate::error::BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(e) = r { result.push(e); } }
        Ok(result)
    }).await.map_err(|e| e.to_string())
}

pub fn archive_token(path: &str) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(path).map_err(|e| format!("read: {}", e))?;
    let hash = Sha256::digest(&bytes);
    let hex = format!("{:x}", hash);
    Ok(hex.chars().rev().take(8).collect::<String>().chars().rev().collect())
}

pub async fn purge_archive(db: &BrainDb, archive_path: &str, confirm_token: &str) -> Result<u64, String> {
    let expected = archive_token(archive_path)?;
    if confirm_token.trim() != expected {
        return Err(format!("Confirmation token mismatch. Expected '{}'.", expected));
    }

    let content = std::fs::read_to_string(archive_path).map_err(|e| format!("read: {}", e))?;
    let hashes: Vec<String> = content.lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter_map(|v| v.get("content_hash").and_then(|x| x.as_str()).map(String::from))
        .collect();

    let deleted = db.with_conn(move |conn| {
        let mut deleted = 0u64;
        for hash in &hashes {
            let count = conn.execute("DELETE FROM nodes WHERE content_hash = ?1", params![hash])
                .unwrap_or(0);
            if count > 0 { deleted += 1; }
        }
        Ok(deleted)
    }).await.map_err(|e| e.to_string())?;

    log::info!("Cold archive PURGED {}: {} rows deleted", archive_path, deleted);
    Ok(deleted)
}

pub async fn import_archive(db: &BrainDb, path: &str) -> Result<u64, String> {
    let content = std::fs::read_to_string(path).map_err(|e| format!("read: {}", e))?;
    let mut imported = 0u64;
    let mut skipped = 0u64;

    for line in content.lines() {
        if line.trim().is_empty() { continue; }
        let v: serde_json::Value = match serde_json::from_str(line) { Ok(v) => v, Err(_) => continue };
        let title = v.get("title").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let content_str = v.get("content").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let domain = v.get("domain").and_then(|x| x.as_str()).unwrap_or("technology").to_string();
        let topic = v.get("topic").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let tags: Vec<String> = v.get("tags").and_then(|x| x.as_array())
            .map(|arr| arr.iter().filter_map(|t| t.as_str().map(String::from)).collect()).unwrap_or_default();
        let node_type = v.get("node_type").and_then(|x| x.as_str()).unwrap_or("reference").to_string();
        let source_type = v.get("source_type").and_then(|x| x.as_str()).unwrap_or("file").to_string();
        let source_url = v.get("source_url").and_then(|x| x.as_str()).map(String::from);

        let input = crate::db::models::CreateNodeInput { title, content: content_str, domain, topic, tags, node_type, source_type, source_url };
        match db.create_node(input).await { Ok(_) => imported += 1, Err(_) => skipped += 1 }
    }
    log::info!("Cold archive import: {} imported, {} skipped", imported, skipped);
    Ok(imported)
}
