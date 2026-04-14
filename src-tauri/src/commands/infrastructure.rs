//! Phase Omega Part VII — Infrastructure Commands
//!
//! Tauri IPC commands for distributed brain management, edge caching,
//! and system health monitoring.

use crate::db::BrainDb;
use crate::distributed::{self, BrainNode, ClusterStatus};
use crate::edge_cache::{self, EdgeCacheExport, EdgeDevice};
use crate::error::BrainError;
use crate::system_health::{self, SystemHealth};
use serde::Deserialize;
use std::sync::Arc;
use tauri::State;

// =========================================================================
// Cluster management
// =========================================================================

#[tauri::command]
pub async fn get_cluster_status(
    db: State<'_, Arc<BrainDb>>,
) -> Result<ClusterStatus, BrainError> {
    distributed::get_cluster_status(&db).await
}

#[derive(Debug, Deserialize)]
pub struct RegisterNodeInput {
    pub name: String,
    pub role: String,
    pub endpoint_url: String,
}

#[tauri::command]
pub async fn register_brain_node(
    db: State<'_, Arc<BrainDb>>,
    input: RegisterNodeInput,
) -> Result<BrainNode, BrainError> {
    distributed::init_distributed(&db).await?;
    distributed::register_node(&db, input.name, input.role, input.endpoint_url).await
}

// =========================================================================
// Edge devices
// =========================================================================

#[tauri::command]
pub async fn get_edge_devices(
    db: State<'_, Arc<BrainDb>>,
) -> Result<Vec<EdgeDevice>, BrainError> {
    edge_cache::get_edge_devices(&db).await
}

#[derive(Debug, Deserialize)]
pub struct RegisterEdgeInput {
    pub device_name: String,
    pub cache_size: Option<u64>,
}

#[tauri::command]
pub async fn register_edge_device(
    db: State<'_, Arc<BrainDb>>,
    input: RegisterEdgeInput,
) -> Result<EdgeDevice, BrainError> {
    edge_cache::register_edge_device(&db, input.device_name, input.cache_size.unwrap_or(1000))
        .await
}

#[tauri::command]
pub async fn compute_edge_cache_cmd(
    db: State<'_, Arc<BrainDb>>,
    device_id: String,
    cache_size: Option<u64>,
) -> Result<Vec<String>, BrainError> {
    edge_cache::compute_edge_cache(&db, device_id, cache_size.unwrap_or(1000)).await
}

#[tauri::command]
pub async fn export_edge_cache_cmd(
    db: State<'_, Arc<BrainDb>>,
    device_id: String,
) -> Result<EdgeCacheExport, BrainError> {
    edge_cache::export_edge_cache(&db, device_id).await
}

// =========================================================================
// System health
// =========================================================================

#[tauri::command]
pub async fn get_system_health(
    db: State<'_, Arc<BrainDb>>,
) -> Result<SystemHealth, BrainError> {
    system_health::get_system_health(&db).await
}
