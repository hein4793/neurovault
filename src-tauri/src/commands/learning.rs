use crate::db::BrainDb;
use crate::error::BrainError;
use crate::learning::{curiosity::CuriosityItem, gaps::KnowledgeGap, missions::ResearchMission};
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub async fn get_knowledge_gaps(db: State<'_, Arc<BrainDb>>) -> Result<Vec<KnowledgeGap>, BrainError> {
    crate::learning::gaps::detect_gaps(&db).await
}

#[tauri::command]
pub async fn get_curiosity_queue(db: State<'_, Arc<BrainDb>>) -> Result<Vec<CuriosityItem>, BrainError> {
    crate::learning::curiosity::generate_curiosity_queue(&db).await
}

#[tauri::command]
pub async fn create_research_mission(
    db: State<'_, Arc<BrainDb>>,
    topic: String,
) -> Result<ResearchMission, BrainError> {
    crate::learning::missions::create_mission(&db, &topic).await
}

#[tauri::command]
pub async fn get_research_missions(db: State<'_, Arc<BrainDb>>) -> Result<Vec<ResearchMission>, BrainError> {
    crate::learning::missions::get_missions(&db).await
}
