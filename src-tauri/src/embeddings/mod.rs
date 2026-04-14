pub mod hnsw;
pub mod pipeline;
pub mod similarity;
pub mod vector_backend;

use crate::error::BrainError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
struct EmbeddingRequest {
    model: String,
    prompt: String,
}

#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
    embedding: Vec<f64>,
}

/// Client for generating embeddings via Ollama's local API.
pub struct OllamaClient {
    base_url: String,
    model: String,
    client: reqwest::Client,
}

impl OllamaClient {
    /// Create a new Ollama client pointing at the given base URL and model.
    pub fn new(base_url: String, model: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            base_url,
            model,
            client,
        }
    }

    /// Check whether Ollama is reachable and responding (fast, 3s timeout).
    pub async fn health_check(&self) -> bool {
        let url = format!("{}/api/tags", self.base_url);
        let check = async {
            match self.client.get(&url).send().await {
                Ok(resp) => resp.status().is_success(),
                Err(_) => false,
            }
        };
        tokio::time::timeout(std::time::Duration::from_secs(3), check)
            .await
            .unwrap_or(false)
    }

    /// Generate an embedding vector for the given text.
    pub async fn generate_embedding(&self, text: &str) -> Result<Vec<f64>, BrainError> {
        // Truncate very long text for embedding (most models handle ~8192 tokens)
        let truncated = crate::truncate_str(text, 8000);

        let resp = self
            .client
            .post(format!("{}/api/embeddings", self.base_url))
            .json(&EmbeddingRequest {
                model: self.model.clone(),
                prompt: truncated.to_string(),
            })
            .send()
            .await
            .map_err(|e| BrainError::Embedding(format!("Ollama request failed: {}", e)))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(BrainError::Embedding(format!(
                "Ollama returned {}: {}",
                status, body
            )));
        }

        let data: EmbeddingResponse = resp
            .json()
            .await
            .map_err(|e| BrainError::Embedding(format!("Failed to parse embedding response: {}", e)))?;

        if data.embedding.is_empty() {
            return Err(BrainError::Embedding(
                "Ollama returned empty embedding vector".to_string(),
            ));
        }

        Ok(data.embedding)
    }

    /// Generate embeddings for multiple texts sequentially.
    #[allow(dead_code)]
    pub async fn batch_embed(&self, texts: &[String]) -> Result<Vec<Vec<f64>>, BrainError> {
        let mut embeddings = Vec::with_capacity(texts.len());
        for text in texts {
            let emb = self.generate_embedding(text).await?;
            embeddings.push(emb);
        }
        Ok(embeddings)
    }
}
