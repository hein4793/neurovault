//! Phase Omega Part IX — Consciousness Layer Commands
//!
//! Tauri IPC commands for the self-model, attention mechanism, and
//! advanced curiosity engine.

use crate::attention::AttentionWindow;
use crate::curiosity_v2::CuriosityTarget;
use crate::db::BrainDb;
use crate::error::BrainError;
use crate::self_model::SelfModel;
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub async fn get_self_model(
    db: State<'_, Arc<BrainDb>>,
) -> Result<Option<SelfModel>, BrainError> {
    crate::self_model::get_self_model(&db).await
}

#[tauri::command]
pub async fn get_attention_window(
    db: State<'_, Arc<BrainDb>>,
) -> Result<AttentionWindow, BrainError> {
    crate::attention::get_focus_window(&db).await
}

#[tauri::command]
pub async fn get_curiosity_targets_v2(
    db: State<'_, Arc<BrainDb>>,
    limit: Option<usize>,
) -> Result<Vec<CuriosityTarget>, BrainError> {
    crate::curiosity_v2::get_curiosity_targets(&db, limit.unwrap_or(20)).await
}
