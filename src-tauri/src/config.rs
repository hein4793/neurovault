use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainConfig {
    pub data_dir: PathBuf,
    pub ollama_url: String,
    pub embedding_model: String,
}

impl Default for BrainConfig {
    fn default() -> Self {
        let data_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".neurovault");

        Self {
            data_dir,
            ollama_url: "http://localhost:11434".to_string(),
            embedding_model: "nomic-embed-text".to_string(),
        }
    }
}

impl BrainConfig {
    /// SQLite database file path (replaces SurrealDB).
    pub fn sqlite_path(&self) -> PathBuf {
        self.data_dir.join("data").join("brain.db")
    }

    /// Obsidian-style vault directory for plain-markdown knowledge files.
    pub fn vault_dir(&self) -> PathBuf {
        self.data_dir.join("vault")
    }

    pub fn export_dir(&self) -> PathBuf {
        self.data_dir.join("export")
    }

    /// Persisted HNSW index file (Phase 1 — fast semantic search).
    pub fn hnsw_index_path(&self) -> PathBuf {
        self.data_dir.join("data").join("hnsw.bin")
    }

    /// HTTP API port for the brain (Phase 1 — MCP bridge).
    pub fn http_api_port(&self) -> u16 {
        std::env::var("CLAUDE_BRAIN_HTTP_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(17777)
    }

    /// Active context file the proactive sidekick writes for Claude Code.
    pub fn active_context_path(&self) -> PathBuf {
        self.export_dir().join("active-context.md")
    }

    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(self.data_dir.join("data"))?;
        std::fs::create_dir_all(self.vault_dir())?;
        std::fs::create_dir_all(self.data_dir.join("cache").join("web"))?;
        std::fs::create_dir_all(self.data_dir.join("logs"))?;
        std::fs::create_dir_all(self.data_dir.join("backups"))?;
        std::fs::create_dir_all(self.export_dir().join("nodes"))?;
        Ok(())
    }
}
