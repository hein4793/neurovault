use crate::db::BrainDb;
use crate::error::BrainError;
use crate::events::{emit_event, BrainEvent};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainSettings {
    pub ollama_url: String,
    pub embedding_model: String,
    pub auto_sync_enabled: bool,
    pub data_dir: String,
    pub llm_provider: String,
    /// Default LLM model — used when a circuit doesn't specify fast/deep.
    pub llm_model: String,
    /// Phase 2.6 — fast model for high-frequency circuits (pattern mining,
    /// decision extraction, summaries, tag extraction). Optimised for speed.
    /// Recommendation: qwen2.5-coder:14b on AMD/Apple silicon, runs at
    /// 40-60 tok/s on a 16GB GPU like the RX 6900 XT.
    #[serde(default = "default_fast_model")]
    pub llm_model_fast: String,
    /// Phase 2.6 — deep model for complex reasoning circuits (synthesis,
    /// hypothesis testing, contradiction detection, cross-domain insights).
    /// Optimised for output quality. Recommendation: qwen2.5-coder:32b at
    /// Q4_K_M with partial GPU offload (~75% of layers on 16GB GPU).
    #[serde(default = "default_deep_model")]
    pub llm_model_deep: String,
    // Autonomy settings
    #[serde(default = "default_true")]
    pub autonomy_enabled: bool,
    #[serde(default = "default_linking_mins")]
    pub autonomy_linking_mins: u64,
    #[serde(default = "default_quality_mins")]
    pub autonomy_quality_mins: u64,
    #[serde(default = "default_learning_mins")]
    pub autonomy_learning_mins: u64,
    #[serde(default = "default_export_mins")]
    pub autonomy_export_mins: u64,
    #[serde(default = "default_max_daily_research")]
    pub autonomy_max_daily_research: u32,
}

fn default_true() -> bool { true }
fn default_linking_mins() -> u64 { 60 }
fn default_quality_mins() -> u64 { 240 }
fn default_learning_mins() -> u64 { 360 }
fn default_export_mins() -> u64 { 30 }
fn default_max_daily_research() -> u32 { 10 }
fn default_fast_model() -> String { "qwen2.5-coder:14b".to_string() }
fn default_deep_model() -> String { "qwen2.5-coder:32b".to_string() }

impl Default for BrainSettings {
    fn default() -> Self {
        Self {
            ollama_url: "http://localhost:11434".to_string(),
            embedding_model: "nomic-embed-text".to_string(),
            auto_sync_enabled: true,
            data_dir: String::new(),
            llm_provider: "ollama".to_string(),
            llm_model: "qwen2.5-coder:14b".to_string(),
            llm_model_fast: "qwen2.5-coder:14b".to_string(),
            llm_model_deep: "qwen2.5-coder:32b".to_string(),
            autonomy_enabled: true,
            autonomy_linking_mins: 60,
            autonomy_quality_mins: 240,
            autonomy_learning_mins: 360,
            autonomy_export_mins: 30,
            autonomy_max_daily_research: 10,
        }
    }
}

fn settings_path(db: &BrainDb) -> std::path::PathBuf {
    db.config.data_dir.join("settings.json")
}

/// Read settings from disk (non-Tauri, for background tasks like autonomy loop)
pub fn load_settings(db: &BrainDb) -> BrainSettings {
    let path = settings_path(db);
    if path.exists() {
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|data| serde_json::from_str(&data).ok())
            .unwrap_or_else(|| {
                let mut s = BrainSettings::default();
                s.data_dir = db.config.data_dir.to_string_lossy().to_string();
                s
            })
    } else {
        let mut s = BrainSettings::default();
        s.data_dir = db.config.data_dir.to_string_lossy().to_string();
        s
    }
}

#[tauri::command]
pub async fn get_settings(db: State<'_, Arc<BrainDb>>) -> Result<BrainSettings, BrainError> {
    let path = settings_path(&db);
    if path.exists() {
        let data = std::fs::read_to_string(&path)
            .map_err(|e| BrainError::Io(e))?;
        let settings: BrainSettings = serde_json::from_str(&data)
            .map_err(|e| BrainError::Serialization(e))?;
        Ok(settings)
    } else {
        let mut settings = BrainSettings::default();
        settings.data_dir = db.config.data_dir.to_string_lossy().to_string();
        settings.ollama_url = db.config.ollama_url.clone();
        settings.embedding_model = db.config.embedding_model.clone();
        Ok(settings)
    }
}

#[tauri::command]
pub async fn update_settings(
    app: tauri::AppHandle,
    db: State<'_, Arc<BrainDb>>,
    settings: BrainSettings,
) -> Result<BrainSettings, BrainError> {
    let path = settings_path(&db);
    let data = serde_json::to_string_pretty(&settings)
        .map_err(|e| BrainError::Serialization(e))?;
    std::fs::write(&path, data)
        .map_err(|e| BrainError::Io(e))?;

    emit_event(&app, BrainEvent::SettingsUpdated { key: "all".to_string() });

    Ok(settings)
}

#[tauri::command]
pub async fn clear_cache(db: State<'_, Arc<BrainDb>>) -> Result<String, BrainError> {
    let cache_dir = db.config.data_dir.join("cache");
    if cache_dir.exists() {
        std::fs::remove_dir_all(&cache_dir)
            .map_err(|e| BrainError::Io(e))?;
        std::fs::create_dir_all(&cache_dir)
            .map_err(|e| BrainError::Io(e))?;
    }
    Ok("Cache cleared".to_string())
}

#[tauri::command]
pub async fn get_brain_version() -> Result<String, BrainError> {
    Ok(env!("CARGO_PKG_VERSION").to_string())
}
