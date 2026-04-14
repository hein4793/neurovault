//! Tauri commands for the federation system (Phase Omega Part VI).

use crate::db::BrainDb;
use crate::error::BrainError;
use crate::federation::{self, FederatedBrain, FederationStatus};
use serde::Deserialize;
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub async fn get_federation_status(
    db: State<'_, Arc<BrainDb>>,
) -> Result<FederationStatus, BrainError> {
    federation::get_federation_status(&db).await
}

#[derive(Debug, Deserialize)]
pub struct RegisterBrainInput {
    pub name: String,
    pub endpoint_url: String,
}

#[tauri::command]
pub async fn register_federated_brain(
    db: State<'_, Arc<BrainDb>>,
    input: RegisterBrainInput,
) -> Result<FederatedBrain, BrainError> {
    federation::register_brain(&db, input.name, input.endpoint_url).await
}

#[derive(Debug, Deserialize)]
pub struct ShareKnowledgeInput {
    pub brain_id: String,
    pub node_ids: Vec<String>,
}

#[tauri::command]
pub async fn share_knowledge_cmd(
    db: State<'_, Arc<BrainDb>>,
    input: ShareKnowledgeInput,
) -> Result<String, BrainError> {
    federation::share_knowledge(&db, &input.brain_id, input.node_ids).await
}

#[tauri::command]
pub async fn sync_with_brain_cmd(
    db: State<'_, Arc<BrainDb>>,
    brain_id: String,
) -> Result<String, BrainError> {
    federation::sync_with_brain(&db, &brain_id).await
}
