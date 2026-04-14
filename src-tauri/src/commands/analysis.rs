use crate::db::BrainDb;
use crate::error::BrainError;
use crate::analysis::{patterns::PatternReport, trends::TrendReport, recommendations::Recommendation};
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub async fn analyze_patterns(db: State<'_, Arc<BrainDb>>) -> Result<PatternReport, BrainError> {
    crate::analysis::patterns::analyze_patterns(&db).await
}

#[tauri::command]
pub async fn analyze_trends(db: State<'_, Arc<BrainDb>>) -> Result<TrendReport, BrainError> {
    crate::analysis::trends::analyze_trends(&db).await
}

#[tauri::command]
pub async fn get_recommendations(db: State<'_, Arc<BrainDb>>) -> Result<Vec<Recommendation>, BrainError> {
    crate::analysis::recommendations::get_recommendations(&db).await
}
