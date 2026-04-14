use crate::db::models::{AutoLinkResult, BrainStats};
use crate::db::BrainDb;
use crate::error::BrainError;
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub async fn get_brain_stats(
    db: State<'_, Arc<BrainDb>>,
) -> Result<BrainStats, BrainError> {
    db.get_brain_stats().await
}

#[tauri::command]
pub async fn auto_link_nodes(
    db: State<'_, Arc<BrainDb>>,
) -> Result<AutoLinkResult, BrainError> {
    db.auto_link_nodes().await
}
