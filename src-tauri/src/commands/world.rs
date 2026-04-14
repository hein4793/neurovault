//! Phase Omega Part III — World Model Tauri commands
//!
//! Exposes the causal world model to the frontend via IPC:
//! - `get_world_entities` — list all world entities
//! - `get_causal_links` — list all causal links
//! - `simulate_scenario` — run a scenario simulation
//! - `get_predictions` — list all predictions

use crate::db::BrainDb;
use crate::error::BrainError;
use crate::temporal::FuturePrediction;
use crate::world_model::{CausalLink, CausalPrediction, WorldEntity};
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub async fn get_world_entities(
    db: State<'_, Arc<BrainDb>>,
) -> Result<Vec<WorldEntity>, BrainError> {
    crate::world_model::get_all_entities(&db).await
}

#[tauri::command]
pub async fn get_causal_links(
    db: State<'_, Arc<BrainDb>>,
) -> Result<Vec<CausalLink>, BrainError> {
    crate::world_model::get_all_links(&db).await
}

#[tauri::command]
pub async fn simulate_scenario_cmd(
    db: State<'_, Arc<BrainDb>>,
    trigger: String,
) -> Result<CausalPrediction, BrainError> {
    crate::world_model::simulate_scenario(&db, &trigger).await
}

#[tauri::command]
pub async fn get_predictions(
    db: State<'_, Arc<BrainDb>>,
) -> Result<Vec<FuturePrediction>, BrainError> {
    crate::temporal::get_predictions(&db).await
}
