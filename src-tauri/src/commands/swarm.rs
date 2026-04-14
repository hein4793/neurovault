//! Tauri commands for the swarm orchestrator (Phase Omega Part II).

use crate::db::BrainDb;
use crate::error::BrainError;
use crate::swarm::{self, SwarmStatus, SwarmTask};
use serde::Deserialize;
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub async fn get_swarm_status(
    db: State<'_, Arc<BrainDb>>,
) -> Result<SwarmStatus, BrainError> {
    // Ensure tables exist before querying
    swarm::init_swarm(&db).await?;
    swarm::get_swarm_status_inner(&db).await
}

#[derive(Debug, Deserialize)]
pub struct CreateSwarmTaskInput {
    pub title: String,
    pub description: Option<String>,
    pub priority: Option<f32>,
    pub dependencies: Option<Vec<String>>,
}

#[tauri::command]
pub async fn create_swarm_task(
    db: State<'_, Arc<BrainDb>>,
    input: CreateSwarmTaskInput,
) -> Result<SwarmTask, BrainError> {
    swarm::init_swarm(&db).await?;
    swarm::create_task(
        &db,
        input.title,
        input.description.unwrap_or_default(),
        input.priority.unwrap_or(0.5),
        input.dependencies.unwrap_or_default(),
        None,
    ).await
}

#[tauri::command]
pub async fn decompose_goal(
    db: State<'_, Arc<BrainDb>>,
    goal: String,
) -> Result<Vec<SwarmTask>, BrainError> {
    swarm::init_swarm(&db).await?;
    swarm::decompose_goal(&db, &goal).await
}

#[tauri::command]
pub async fn get_swarm_tasks(
    db: State<'_, Arc<BrainDb>>,
) -> Result<Vec<SwarmTask>, BrainError> {
    swarm::init_swarm(&db).await?;
    swarm::get_tasks(&db).await
}
