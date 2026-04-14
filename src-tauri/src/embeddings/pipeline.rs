use crate::db::BrainDb;
use crate::embeddings::OllamaClient;
use crate::error::BrainError;
use rusqlite::params;
use std::sync::Arc;

pub async fn run_embedding_pipeline(db: Arc<BrainDb>, ollama_url: String, model: String) {
    let client = OllamaClient::new(ollama_url, model);
    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    let mut consecutive_errors = 0u32;

    loop {
        if !client.health_check().await {
            log::warn!("Embedding pipeline: Ollama not reachable, retrying in 60s");
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            continue;
        }
        match process_batch(&db, &client).await {
            Ok(count) => {
                consecutive_errors = 0;
                if count > 0 { log::info!("Embedding pipeline: embedded {} nodes this cycle", count); }
            }
            Err(e) => {
                consecutive_errors += 1;
                let backoff = std::cmp::min(30 * consecutive_errors as u64, 300);
                log::error!("Embedding pipeline error (retry in {}s): {}", backoff, e);
                tokio::time::sleep(std::time::Duration::from_secs(backoff)).await;
                continue;
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
    }
}

async fn process_batch(db: &BrainDb, client: &OllamaClient) -> Result<usize, BrainError> {
    // Find nodes without embeddings
    let nodes: Vec<(String, String, String)> = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT n.id, n.title, n.summary FROM nodes n \
             LEFT JOIN embeddings e ON e.node_id = n.id \
             WHERE e.node_id IS NULL LIMIT 50"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(n) = r { result.push(n); } }
        Ok(result)
    }).await?;

    if nodes.is_empty() { return Ok(0); }
    log::info!("Embedding pipeline: found {} nodes without embeddings", nodes.len());

    let mut embedded_count = 0usize;

    for (id, title, summary) in &nodes {
        let text = format!("{} {}", title, summary);
        match client.generate_embedding(&text).await {
            Ok(embedding) => {
                let id_clone = id.clone();
                let emb = embedding.clone();
                match db.with_conn(move |conn| {
                    let blob: Vec<u8> = emb.iter().flat_map(|f| f.to_le_bytes()).collect();
                    let dim = emb.len();
                    conn.execute(
                        "INSERT OR REPLACE INTO embeddings (node_id, vector, dimension) VALUES (?1, ?2, ?3)",
                        params![id_clone, blob, dim],
                    ).map_err(|e| BrainError::Database(e.to_string()))
                }).await {
                    Ok(_) => {
                        embedded_count += 1;
                        db.hnsw.write().await.mark_dirty();
                    }
                    Err(e) => log::error!("Failed to store embedding for {}: {}", id, e),
                }
            }
            Err(e) => log::error!("Failed to generate embedding for {}: {}", id, e),
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    Ok(embedded_count)
}

pub async fn backfill_embeddings(db: &BrainDb, client: &OllamaClient) -> (u64, u64) {
    let mut generated = 0u64;
    let mut failed = 0u64;

    let nodes: Vec<(String, String, String, String)> = match db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT n.id, n.title, n.topic, n.content FROM nodes n \
             LEFT JOIN embeddings e ON e.node_id = n.id \
             WHERE e.node_id IS NULL"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(n) = r { result.push(n); } }
        Ok(result)
    }).await {
        Ok(n) => n,
        Err(e) => {
            log::error!("Failed to fetch nodes for embedding backfill: {}", e);
            return (0, 0);
        }
    };

    for (id, title, topic, content) in &nodes {
        let embed_text = format!("{}\n{}\n{}", title, topic, crate::truncate_str(content, 2000));
        match client.generate_embedding(&embed_text).await {
            Ok(embedding) => {
                if let Err(e) = db.update_node_embedding(id, embedding).await {
                    log::warn!("Failed to save embedding for {}: {}", id, e);
                    failed += 1;
                } else {
                    generated += 1;
                }
            }
            Err(e) => { log::warn!("Failed to generate embedding for {}: {}", title, e); failed += 1; }
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    }

    log::info!("Embedding backfill complete: {} generated, {} failed", generated, failed);
    (generated, failed)
}

#[allow(dead_code)]
pub async fn embed_node(db: &BrainDb, client: &OllamaClient, node_id: &str) -> Result<(), BrainError> {
    let id = node_id.to_string();
    let (title, topic, content) = db.with_conn(move |conn| {
        conn.query_row(
            "SELECT title, topic, content FROM nodes WHERE id = ?1",
            params![id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)),
        ).map_err(|e| BrainError::NotFound(format!("Node not found: {}", e)))
    }).await?;

    let embed_text = format!("{}\n{}\n{}", title, topic, crate::truncate_str(&content, 2000));
    let embedding = client.generate_embedding(&embed_text).await?;
    db.update_node_embedding(node_id, embedding).await?;
    Ok(())
}
