// MCP client scaffold — Phase 1.1b shipped a Node-based MCP server, this
// Rust-side helper is kept for future native MCP integration.
#![allow(dead_code)]
use crate::error::BrainError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolResult {
    pub content: String,
    pub source: String,
}

/// Simple HTTP-based MCP-like client that can fetch documentation
pub struct McpClient {
    http: reqwest::Client,
}

impl McpClient {
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::new(),
        }
    }

    /// Fetch documentation from a URL (simplified MCP-like query)
    pub async fn fetch_docs(&self, url: &str) -> Result<McpToolResult, BrainError> {
        let resp = self.http
            .get(url)
            .header("User-Agent", "ClaudeBrain/1.0")
            .send()
            .await
            .map_err(|e| BrainError::Ingestion(format!("MCP fetch failed: {}", e)))?;

        let text = resp.text().await
            .map_err(|e| BrainError::Ingestion(format!("MCP read failed: {}", e)))?;

        // Convert HTML to text if needed
        let content = if text.contains("<html") || text.contains("<HTML") {
            html2md::parse_html(&text)
        } else {
            text
        };

        Ok(McpToolResult {
            content,
            source: url.to_string(),
        })
    }
}
