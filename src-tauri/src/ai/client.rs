use crate::error::BrainError;
use crate::power_telemetry;
use serde::{Deserialize, Serialize};
use std::time::Instant;

#[derive(Debug, Serialize)]
struct OllamaGenerateRequest {
    model: String,
    prompt: String,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaOptions>,
}

#[derive(Debug, Serialize)]
struct OllamaOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    num_ctx: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct OllamaGenerateResponse {
    response: String,
}

#[derive(Debug, Serialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<AnthropicMessage>,
}

#[derive(Debug, Deserialize)]
struct AnthropicContent {
    text: String,
}

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContent>,
}

pub struct LlmClient {
    provider: String,
    model: String,
    ollama_url: String,
    api_key: Option<String>,
    client: reqwest::Client,
    /// Backend tag attached to power-telemetry rows for this client.
    /// When None, falls back to provider-based inference in `backend_id()`.
    /// Set by the factory when routing through the CPU-only daemon so
    /// the inference_log correctly attributes energy to `ollama-cpu`.
    backend_tag: Option<&'static str>,
}

/// System prompt that gives the model context about the brain.
const BRAIN_SYSTEM_PROMPT: &str = "You are the AI core of a personal knowledge brain — a living, growing knowledge system. \
You synthesize knowledge, extract patterns, generate insights, and help the user understand their data. \
Be precise, technical, and information-dense. Never be vague or use filler phrases. \
Always cite specific concepts, tools, or patterns. Respond concisely.";

impl LlmClient {
    pub fn new(provider: &str, model: &str, ollama_url: &str, api_key: Option<String>) -> Self {
        Self {
            provider: provider.to_string(),
            model: model.to_string(),
            ollama_url: ollama_url.to_string(),
            api_key,
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(300))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            backend_tag: None,
        }
    }

    /// Tag this client's calls with a specific backend id for telemetry
    /// (e.g. `"ollama-cpu"` when routing through the CPU-only daemon).
    /// Returns Self to support fluent factory code.
    pub fn with_backend_tag(mut self, tag: &'static str) -> Self {
        self.backend_tag = Some(tag);
        self
    }

    /// Model id this client will send generate requests against.
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Backend identifier used in the telemetry log. Uses the explicit tag
    /// if set (Phase 2 routing), else falls back to provider inference.
    fn backend_id(&self) -> &'static str {
        if let Some(tag) = self.backend_tag {
            return tag;
        }
        match self.provider.as_str() {
            "anthropic" => "anthropic-api",
            _ => "ollama-vulkan",
        }
    }

    /// Record a finished inference via the global telemetry accessor.
    /// No-op when telemetry hasn't been initialized (e.g. unit tests).
    /// Detached so callers don't pay the DB round-trip in the hot path.
    fn record_call(&self, duration_ms: u64, prompt_len: usize, response_len: usize) {
        let backend = self.backend_id().to_string();
        let model = self.model.clone();
        // Rough token estimate: ~4 chars per token. Accurate tokenization
        // would need the model's tokenizer; we accept ~15% error for the
        // sake of keeping the recorder backend-agnostic.
        let tokens_in = (prompt_len / 4) as u32;
        let tokens_out = (response_len / 4) as u32;
        tokio::spawn(async move {
            power_telemetry::record_inference_global(
                &backend, &model, tokens_in, tokens_out, duration_ms,
            )
            .await;
        });
    }

    pub async fn generate(&self, prompt: &str, max_tokens: u32) -> Result<String, BrainError> {
        match self.provider.as_str() {
            "anthropic" => self.generate_anthropic(prompt, max_tokens).await,
            _ => self.generate_ollama(prompt, Some(max_tokens)).await,
        }
    }

    async fn generate_ollama(&self, prompt: &str, max_tokens: Option<u32>) -> Result<String, BrainError> {
        let req = OllamaGenerateRequest {
            model: self.model.clone(),
            prompt: prompt.to_string(),
            stream: false,
            system: Some(BRAIN_SYSTEM_PROMPT.to_string()),
            options: Some(OllamaOptions {
                num_ctx: Some(8192),
                temperature: Some(0.3),
                num_predict: max_tokens,
            }),
        };

        // Retry up to 2 times on 404 (model loading race)
        let mut last_err = String::new();
        let started = Instant::now();
        for attempt in 0..3 {
            let resp = self.client
                .post(format!("{}/api/generate", self.ollama_url))
                .json(&req)
                .send()
                .await
                .map_err(|e| BrainError::Embedding(format!("Ollama generate failed: {}", e)))?;

            if resp.status().is_success() {
                let data: OllamaGenerateResponse = resp.json().await
                    .map_err(|e| BrainError::Embedding(format!("Failed to parse Ollama response: {}", e)))?;
                self.record_call(
                    started.elapsed().as_millis() as u64,
                    prompt.len(),
                    data.response.len(),
                );
                return Ok(data.response);
            }

            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            last_err = format!("Ollama returned {} (model={}): {}", status, self.model, body);
            log::warn!("LLM attempt {}/3 failed: {}", attempt + 1, last_err);

            if attempt < 2 {
                // Pull the model if 404 — Ollama may need to load it
                if status.as_u16() == 404 {
                    log::info!("Attempting to pull model '{}' ...", self.model);
                    let _ = self.client
                        .post(format!("{}/api/pull", self.ollama_url))
                        .json(&serde_json::json!({ "name": self.model, "stream": false }))
                        .send()
                        .await;
                }
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        }

        Err(BrainError::Embedding(last_err))
    }

    async fn generate_anthropic(&self, prompt: &str, max_tokens: u32) -> Result<String, BrainError> {
        let api_key = self.api_key.as_ref()
            .ok_or_else(|| BrainError::Embedding("Anthropic API key not configured".to_string()))?;

        let started = Instant::now();
        let resp = self.client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&AnthropicRequest {
                model: self.model.clone(),
                max_tokens,
                messages: vec![AnthropicMessage {
                    role: "user".to_string(),
                    content: prompt.to_string(),
                }],
            })
            .send()
            .await
            .map_err(|e| BrainError::Embedding(format!("Anthropic request failed: {}", e)))?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(BrainError::Embedding(format!("Anthropic error: {}", body)));
        }

        let data: AnthropicResponse = resp.json().await
            .map_err(|e| BrainError::Embedding(format!("Failed to parse Anthropic response: {}", e)))?;

        let text = data.content.first().map(|c| c.text.clone()).unwrap_or_default();
        self.record_call(started.elapsed().as_millis() as u64, prompt.len(), text.len());
        Ok(text)
    }

    /// Summarize content into a concise, information-dense summary.
    pub async fn summarize(&self, content: &str) -> Result<String, BrainError> {
        let prompt = format!(
            "Create a precise, information-dense summary of this knowledge.\n\n\
             CONTENT:\n{}\n\n\
             Write 2-4 sentences capturing: the core concept, key technical details, and practical significance.\n\
             Be specific — use names, numbers, and technical terms. Never say \"this document discusses...\"",
            crate::truncate_str(content, 6000)
        );
        self.generate(&prompt, 500).await
    }

    /// Extract precise, categorized tags from content.
    pub async fn extract_tags(&self, content: &str) -> Result<Vec<String>, BrainError> {
        let prompt = format!(
            "Extract 5-10 precise tags from this content. Include:\n\
             - Primary technology/concept names\n\
             - Domain categories\n\
             - Specific patterns or techniques\n\
             - Related fields\n\n\
             CONTENT:\n{}\n\n\
             Return ONLY lowercase comma-separated tags. Example: typescript, react, state-management, hooks, frontend",
            crate::truncate_str(content, 5000)
        );
        let response = self.generate(&prompt, 200).await?;
        Ok(response.split(',')
            .map(|t| t.trim().to_lowercase())
            .filter(|t| !t.is_empty() && t.len() < 50 && t.len() > 1)
            .collect())
    }
}
