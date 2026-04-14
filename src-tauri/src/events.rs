use serde::{Deserialize, Serialize};
use tauri::Emitter;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BrainEvent {
    NodeCreated { id: String, title: String },
    NodeUpdated { id: String, title: String },
    NodeDeleted { id: String },
    EdgeCreated { id: String, source: String, target: String },
    EdgeDeleted { id: String },
    IngestionStarted { source: String },
    IngestionCompleted { source: String, nodes_created: u32 },
    IngestionFailed { source: String, error: String },
    SearchPerformed { query: String, results: u32 },
    ImportCompleted { source: String, nodes_imported: u32 },
    AutoLinkCompleted { created: u64, total_nodes: u64 },
    ResearchStarted { topic: String },
    ResearchCompleted { topic: String, nodes_created: u32 },
    SettingsUpdated { key: String },
    // Autonomy events
    AutonomyTaskStarted { task: String },
    AutonomyTaskCompleted { task: String, result: String, duration_ms: u64 },
    AutonomyTaskFailed { task: String, error: String },
    ActiveLearningCompleted { topics_researched: u32, nodes_created: u32 },
    BriefingUpdated { iq: f64, total_nodes: u64 },
}

/// Emit a brain event to the frontend via Tauri's event system
pub fn emit_event(app: &tauri::AppHandle, event: BrainEvent) {
    if let Err(e) = app.emit("brain-event", &event) {
        log::warn!("Failed to emit brain event: {}", e);
    }
}
