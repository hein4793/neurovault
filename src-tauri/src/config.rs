use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainConfig {
    pub data_dir: PathBuf,
    pub ollama_url: String,
    pub embedding_model: String,
    /// Whether the first-run onboarding wizard has been completed.
    #[serde(default)]
    pub setup_completed: bool,
    /// User-chosen name for their brain instance.
    #[serde(default = "default_brain_name")]
    pub brain_name: String,
    /// Watch AI assistant chat history directories for auto-ingestion.
    #[serde(default = "default_true")]
    pub enable_ai_assistant_sync: bool,
    /// Watch user-configured directories for file changes.
    #[serde(default)]
    pub enable_file_watcher: bool,
    /// User-configurable directories to watch for knowledge ingestion.
    #[serde(default)]
    pub watched_paths: Vec<PathBuf>,
}

fn default_brain_name() -> String {
    "My Brain".to_string()
}
fn default_true() -> bool {
    true
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
            setup_completed: false,
            brain_name: "My Brain".to_string(),
            enable_ai_assistant_sync: true,
            enable_file_watcher: false,
            watched_paths: Vec::new(),
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
        std::env::var("NEUROVAULT_HTTP_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(17777)
    }

    /// Active context file the proactive sidekick writes for your AI assistant.
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
