use crate::db::BrainDb;
use crate::error::BrainError;
use crate::events::{emit_event, BrainEvent};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;
use std::path::PathBuf;

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
    // Onboarding fields
    #[serde(default)]
    pub setup_completed: bool,
    #[serde(default = "default_brain_name")]
    pub brain_name: String,
    #[serde(default = "default_true")]
    pub enable_ai_assistant_sync: bool,
    #[serde(default)]
    pub enable_file_watcher: bool,
    #[serde(default)]
    pub watched_paths: Vec<String>,
}

fn default_true() -> bool { true }
fn default_brain_name() -> String { "My Brain".to_string() }
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
            setup_completed: false,
            brain_name: "My Brain".to_string(),
            enable_ai_assistant_sync: true,
            enable_file_watcher: false,
            watched_paths: Vec::new(),
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

// ===== Onboarding wizard commands =====

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupStatus {
    pub setup_completed: bool,
}

/// Returns whether the first-run onboarding wizard has been completed.
/// Reads from settings.json; defaults to false if the file doesn't exist.
#[tauri::command]
pub async fn get_setup_status(db: State<'_, Arc<BrainDb>>) -> Result<SetupStatus, BrainError> {
    let settings = load_settings(&db);
    Ok(SetupStatus {
        setup_completed: settings.setup_completed,
    })
}

/// Marks onboarding as complete and persists optional configuration
/// chosen during the wizard (brain name, watched paths, toggles).
#[tauri::command]
pub async fn complete_setup(
    app: tauri::AppHandle,
    db: State<'_, Arc<BrainDb>>,
    brain_name: String,
    enable_ai_assistant_sync: bool,
    enable_file_watcher: bool,
    watched_paths: Vec<String>,
) -> Result<SetupStatus, BrainError> {
    let mut settings = load_settings(&db);
    settings.setup_completed = true;
    settings.brain_name = brain_name;
    settings.enable_ai_assistant_sync = enable_ai_assistant_sync;
    settings.enable_file_watcher = enable_file_watcher;
    settings.watched_paths = watched_paths;

    let path = settings_path(&db);
    let data = serde_json::to_string_pretty(&settings)
        .map_err(BrainError::Serialization)?;
    std::fs::write(&path, data).map_err(BrainError::Io)?;

    emit_event(&app, BrainEvent::SettingsUpdated {
        key: "setup_completed".to_string(),
    });

    Ok(SetupStatus { setup_completed: true })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaStatus {
    pub reachable: bool,
    pub models: Vec<OllamaModelInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaModelInfo {
    pub name: String,
    pub size: u64,
}

/// Checks if Ollama is running and lists available models.
#[tauri::command]
pub async fn check_ollama_status(
    db: State<'_, Arc<BrainDb>>,
) -> Result<OllamaStatus, BrainError> {
    let url = format!("{}/api/tags", db.config.ollama_url);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    #[derive(Debug, Deserialize)]
    struct TagsResp {
        models: Vec<TagModel>,
    }
    #[derive(Debug, Deserialize)]
    struct TagModel {
        name: String,
        #[serde(default)]
        size: u64,
    }

    match client.get(&url).send().await {
        Ok(resp) if resp.status().is_success() => {
            let tags: TagsResp = resp.json().await.unwrap_or(TagsResp { models: Vec::new() });
            let models = tags
                .models
                .into_iter()
                .map(|t| OllamaModelInfo {
                    name: t.name,
                    size: t.size,
                })
                .collect();
            Ok(OllamaStatus {
                reachable: true,
                models,
            })
        }
        _ => Ok(OllamaStatus {
            reachable: false,
            models: Vec::new(),
        }),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullProgress {
    pub status: String,
    pub completed: bool,
    pub error: Option<String>,
}

/// Pulls an Ollama model by name. This is a blocking call that streams
/// the pull progress and returns a final status.
#[tauri::command]
pub async fn pull_ollama_model(
    db: State<'_, Arc<BrainDb>>,
    model_name: String,
) -> Result<PullProgress, BrainError> {
    let url = format!("{}/api/pull", db.config.ollama_url);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    let body = serde_json::json!({
        "name": model_name,
        "stream": false
    });

    match client.post(&url).json(&body).send().await {
        Ok(resp) if resp.status().is_success() => {
            // Non-streaming: Ollama returns a final JSON with status
            let text = resp.text().await.unwrap_or_default();
            // The response may contain multiple JSON lines; take the last
            let last_line = text.lines().last().unwrap_or("");
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(last_line) {
                let status = val["status"].as_str().unwrap_or("unknown").to_string();
                let error = val["error"].as_str().map(|s| s.to_string());
                Ok(PullProgress {
                    status,
                    completed: error.is_none(),
                    error,
                })
            } else {
                Ok(PullProgress {
                    status: "completed".to_string(),
                    completed: true,
                    error: None,
                })
            }
        }
        Ok(resp) => {
            let err_text = resp.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            Ok(PullProgress {
                status: "failed".to_string(),
                completed: false,
                error: Some(err_text),
            })
        }
        Err(e) => Ok(PullProgress {
            status: "failed".to_string(),
            completed: false,
            error: Some(format!("Connection failed: {}", e)),
        }),
    }
}

/// Detect default AI assistant chat directories (if they exist).
#[tauri::command]
pub async fn detect_ai_assistant_dirs() -> Result<Vec<String>, BrainError> {
    let mut dirs_found = Vec::new();
    if let Some(home) = dirs::home_dir() {
        // Check for Claude-style projects directory
        let claude_dir = home.join(".claude").join("projects");
        if claude_dir.exists() {
            dirs_found.push(claude_dir.to_string_lossy().to_string());
        }
        // Check for Copilot chat directory
        let copilot_dir = home.join(".config").join("github-copilot");
        if copilot_dir.exists() {
            dirs_found.push(copilot_dir.to_string_lossy().to_string());
        }
    }
    Ok(dirs_found)
}
