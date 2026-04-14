use crate::db::models::*;
use crate::db::BrainDb;
use crate::embeddings::OllamaClient;
use crate::error::BrainError;
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub async fn search_nodes(
    db: State<'_, Arc<BrainDb>>,
    query: String,
) -> Result<Vec<SearchResult>, BrainError> {
    db.search_nodes(&query).await
}

#[tauri::command]
pub async fn semantic_search(
    db: State<'_, Arc<BrainDb>>,
    query: String,
    _limit: Option<u32>,
) -> Result<Vec<SearchResult>, BrainError> {
    let limit = _limit.unwrap_or(20) as usize;

    // Try vector search via Ollama first.
    let client = OllamaClient::new(
        db.config.ollama_url.clone(),
        db.config.embedding_model.clone(),
    );

    if !client.health_check().await {
        log::warn!("Ollama unavailable for semantic search, falling back to text search");
        return db.search_nodes(&query).await;
    }

    let query_emb = match client.generate_embedding(&query).await {
        Ok(emb) => emb,
        Err(e) => {
            log::warn!(
                "Failed to generate query embedding, falling back to text search: {}",
                e
            );
            return db.search_nodes(&query).await;
        }
    };

    // Use client-side vector search via HNSW index
    let query_emb_fallback = query_emb.clone();
    let vector_results = db.vector_search(query_emb, limit).await;

    match vector_results {
        Ok(results) if !results.is_empty() => {
            log::info!(
                "Semantic search for '{}' returned {} results",
                query,
                results.len()
            );
            Ok(results)
        }
        Ok(_) => {
            log::info!("Vector search returned no results, trying client-side fallback");
            let fallback_results = db.vector_search(query_emb_fallback, limit).await?;
            if fallback_results.is_empty() {
                log::info!("No vector results, falling back to text search");
                db.search_nodes(&query).await
            } else {
                Ok(fallback_results)
            }
        }
        Err(e) => {
            log::warn!(
                "Vector search failed, falling back to text search: {}",
                e
            );
            db.search_nodes(&query).await
        }
    }
}
