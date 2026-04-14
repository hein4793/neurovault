//! Tauri commands for inspecting installed Ollama models.
//!
//! Phase 3.1 — drives the multi-model UI panel. The frontend uses
//! `list_installed_models` to populate the fast/deep dropdowns with the
//! actual model names available on the user's machine, so they don't
//! type strings in by hand.

use crate::db::BrainDb;
use crate::error::BrainError;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledModel {
    pub name: String,
    /// Bytes — Ollama returns this as `size` in the API.
    pub size: u64,
    /// RFC3339 modified timestamp.
    pub modified_at: String,
    /// Best-effort family detection ("qwen", "llama", "mistral", "deepseek"...) for grouping in UI.
    pub family: String,
    /// Best-effort param-count parse ("14b", "32b", "7b"...) for sorting.
    pub size_label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledModelsResponse {
    pub ollama_url: String,
    pub reachable: bool,
    pub models: Vec<InstalledModel>,
}

/// Calls `GET {ollama_url}/api/tags` and returns the list of installed
/// models. If Ollama is unreachable, returns `reachable=false` with an
/// empty list — the UI handles this by showing a hint to start Ollama.
#[tauri::command]
pub async fn list_installed_models(
    db: State<'_, Arc<BrainDb>>,
) -> Result<InstalledModelsResponse, BrainError> {
    let url = format!("{}/api/tags", db.config.ollama_url);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    #[derive(Debug, Deserialize)]
    struct OllamaTagsResponse {
        models: Vec<OllamaTag>,
    }
    #[derive(Debug, Deserialize)]
    struct OllamaTag {
        name: String,
        #[serde(default)]
        size: u64,
        #[serde(default)]
        modified_at: String,
    }

    match client.get(&url).send().await {
        Ok(resp) if resp.status().is_success() => {
            let tags: OllamaTagsResponse = match resp.json().await {
                Ok(t) => t,
                Err(_) => {
                    return Ok(InstalledModelsResponse {
                        ollama_url: db.config.ollama_url.clone(),
                        reachable: true,
                        models: Vec::new(),
                    });
                }
            };

            let models: Vec<InstalledModel> = tags
                .models
                .into_iter()
                .map(|t| {
                    let family = detect_family(&t.name);
                    let size_label = detect_size_label(&t.name);
                    InstalledModel {
                        name: t.name,
                        size: t.size,
                        modified_at: t.modified_at,
                        family,
                        size_label,
                    }
                })
                .collect();

            Ok(InstalledModelsResponse {
                ollama_url: db.config.ollama_url.clone(),
                reachable: true,
                models,
            })
        }
        _ => Ok(InstalledModelsResponse {
            ollama_url: db.config.ollama_url.clone(),
            reachable: false,
            models: Vec::new(),
        }),
    }
}

/// Best-effort family detection from a model tag like "qwen2.5-coder:14b"
fn detect_family(name: &str) -> String {
    let lower = name.to_lowercase();
    for family in [
        "qwen2.5-coder", "qwen2.5", "qwen3", "qwen", "deepseek-coder", "deepseek",
        "llama3.3", "llama3.2", "llama3.1", "llama3", "llama2", "llama",
        "mistral", "mixtral", "phi3", "phi4", "phi", "gemma3", "gemma2", "gemma",
        "neural-chat", "codellama", "starcoder", "command-r", "yi",
        "nomic-embed-text", "mxbai-embed", "bge",
    ] {
        if lower.starts_with(family) || lower.contains(&format!("/{}", family)) {
            return family.to_string();
        }
    }
    "other".to_string()
}

/// Extract a size label like "14b" from "qwen2.5-coder:14b". Falls back
/// to the part after the colon if no recognised pattern.
fn detect_size_label(name: &str) -> String {
    let after_colon = name.split(':').nth(1).unwrap_or("");
    let lower = after_colon.to_lowercase();
    // Look for NNb / NNNb / NN.Nb patterns (e.g. "14b", "1.5b", "70b")
    let mut chars = lower.chars().peekable();
    let mut buf = String::new();
    while let Some(&c) = chars.peek() {
        if c.is_ascii_digit() || c == '.' {
            buf.push(c);
            chars.next();
        } else {
            break;
        }
    }
    if !buf.is_empty() {
        if let Some(&c) = chars.peek() {
            if c == 'b' || c == 'B' {
                buf.push('b');
                return buf;
            }
        }
        return buf;
    }
    after_colon.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_qwen_family() {
        assert_eq!(detect_family("qwen2.5-coder:14b"), "qwen2.5-coder");
        assert_eq!(detect_family("qwen2.5:7b"), "qwen2.5");
    }

    #[test]
    fn detects_llama_family() {
        assert_eq!(detect_family("llama3.1:8b"), "llama3.1");
        assert_eq!(detect_family("llama3:70b"), "llama3");
    }

    #[test]
    fn detects_deepseek_family() {
        assert_eq!(detect_family("deepseek-coder-v2:16b"), "deepseek-coder");
    }

    #[test]
    fn extracts_size_labels() {
        assert_eq!(detect_size_label("qwen2.5-coder:14b"), "14b");
        assert_eq!(detect_size_label("llama3:70b"), "70b");
        assert_eq!(detect_size_label("phi3:3.8b"), "3.8b");
        assert_eq!(detect_size_label("mistral:latest"), "latest");
    }
}
