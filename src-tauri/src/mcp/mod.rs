pub mod client;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConnection {
    pub name: String,
    pub status: String, // "connected", "disconnected", "error"
    pub server_type: String, // "context7", "github", "websearch"
    pub last_used: Option<String>,
}
