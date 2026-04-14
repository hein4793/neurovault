use crate::db::models::*;
use crate::db::BrainDb;
use crate::embeddings::similarity::{DuplicatePair, SimilarNode};
use crate::embeddings::OllamaClient;
use crate::error::BrainError;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingStatus {
    pub total_nodes: u64,
    pub nodes_with_embeddings: u64,
    pub ollama_available: bool,
}

/// Statistics about the embedding pipeline's progress.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingStats {
    pub total_nodes: u64,
    pub embedded_nodes: u64,
    pub pending_nodes: u64,
    pub ollama_connected: bool,
}

/// Find nodes most similar to the given node, ranked by cosine similarity.
#[tauri::command]
pub async fn find_similar_nodes(
    db: State<'_, Arc<BrainDb>>,
    node_id: String,
    threshold: Option<f64>,
    limit: Option<usize>,
) -> Result<Vec<SimilarNode>, BrainError> {
    crate::embeddings::similarity::find_similar(
        &db,
        &node_id,
        threshold.unwrap_or(0.5),
        limit.unwrap_or(10),
    )
    .await
}

/// Get statistics about the embedding pipeline's current state.
#[tauri::command]
pub async fn get_embedding_stats(
    db: State<'_, Arc<BrainDb>>,
) -> Result<EmbeddingStats, BrainError> {
    // Total node count and embedded node count
    let (total_nodes, embedded_nodes) = db.with_conn(|conn| -> Result<(u64, u64), BrainError> {
        let total: i64 = conn.query_row(
            "SELECT COUNT(*) FROM nodes", [], |row| row.get(0)
        ).unwrap_or(0);

        let embedded: i64 = conn.query_row(
            "SELECT COUNT(*) FROM embeddings",
            [], |row| row.get(0)
        ).unwrap_or(0);

        Ok((total as u64, embedded as u64))
    }).await?;

    let pending_nodes = total_nodes.saturating_sub(embedded_nodes);

    // Check Ollama connectivity.
    let client = OllamaClient::new(
        db.config.ollama_url.clone(),
        db.config.embedding_model.clone(),
    );
    let ollama_connected = client.health_check().await;

    Ok(EmbeddingStats {
        total_nodes,
        embedded_nodes,
        pending_nodes,
        ollama_connected,
    })
}

/// Get embedding status overview including Ollama availability.
#[tauri::command]
pub async fn get_embedding_status(db: State<'_, Arc<BrainDb>>) -> Result<EmbeddingStatus, BrainError> {
    let (total, with_emb) = db.with_conn(|conn| -> Result<(u64, u64), BrainError> {
        let total: i64 = conn.query_row(
            "SELECT COUNT(*) FROM nodes", [], |row| row.get(0)
        ).unwrap_or(0);

        let with_emb: i64 = conn.query_row(
            "SELECT COUNT(*) FROM embeddings",
            [], |row| row.get(0)
        ).unwrap_or(0);

        Ok((total as u64, with_emb as u64))
    }).await?;

    let client = OllamaClient::new(
        db.config.ollama_url.clone(),
        db.config.embedding_model.clone(),
    );
    let available = client.health_check().await;

    Ok(EmbeddingStatus {
        total_nodes: total,
        nodes_with_embeddings: with_emb,
        ollama_available: available,
    })
}

/// Trigger a full embedding backfill for all nodes missing embeddings.
/// Returns (generated_count, failed_count).
#[tauri::command]
pub async fn generate_embeddings(db: State<'_, Arc<BrainDb>>) -> Result<(u64, u64), BrainError> {
    let client = OllamaClient::new(
        db.config.ollama_url.clone(),
        db.config.embedding_model.clone(),
    );

    if !client.health_check().await {
        return Err(BrainError::Embedding(
            "Ollama is not available. Make sure it's running.".to_string(),
        ));
    }

    let (generated, failed) = crate::embeddings::pipeline::backfill_embeddings(&db, &client).await;
    Ok((generated, failed))
}

/// Scan all nodes for potential duplicates based on embedding similarity.
#[tauri::command]
pub async fn scan_duplicates(
    db: State<'_, Arc<BrainDb>>,
    threshold: Option<f64>,
) -> Result<Vec<DuplicatePair>, BrainError> {
    crate::embeddings::similarity::detect_duplicates(&db, threshold.unwrap_or(0.85)).await
}

/// Semantic search using vector embeddings with fallback to text search.
#[tauri::command]
pub async fn semantic_search_v2(
    db: State<'_, Arc<BrainDb>>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<SearchResult>, BrainError> {
    let client = OllamaClient::new(
        db.config.ollama_url.clone(),
        db.config.embedding_model.clone(),
    );

    // Try vector search first
    match client.generate_embedding(&query).await {
        Ok(query_embedding) => db.vector_search(query_embedding, limit.unwrap_or(20)).await,
        Err(_) => {
            // Fall back to text search if Ollama is unavailable
            db.search_nodes(&query).await
        }
    }
}
