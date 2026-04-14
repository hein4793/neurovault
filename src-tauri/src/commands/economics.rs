//! Tauri commands for the economic autonomy system (Phase Omega Part VIII).

use crate::db::BrainDb;
use crate::economics::{self, ComputeCost, EconomicReport, RevenueEvent};
use crate::error::BrainError;
use serde::Deserialize;
use std::sync::Arc;
use tauri::State;

#[derive(Debug, Deserialize)]
pub struct RecordRevenueInput {
    pub source: String,
    pub amount: f64,
    pub currency: Option<String>,
    pub description: String,
    pub attributed_to: Option<String>,
}

#[tauri::command]
pub async fn record_revenue(
    db: State<'_, Arc<BrainDb>>,
    input: RecordRevenueInput,
) -> Result<RevenueEvent, BrainError> {
    economics::record_revenue(
        &db,
        input.source,
        input.amount,
        input.currency.unwrap_or_else(|| "ZAR".to_string()),
        input.description,
        input.attributed_to,
    )
    .await
}

#[derive(Debug, Deserialize)]
pub struct RecordCostInput {
    pub cost_type: String,
    pub amount: f64,
    pub description: String,
}

#[tauri::command]
pub async fn record_cost(
    db: State<'_, Arc<BrainDb>>,
    input: RecordCostInput,
) -> Result<ComputeCost, BrainError> {
    economics::record_cost(&db, input.cost_type, input.amount, input.description).await
}

#[tauri::command]
pub async fn get_economic_report(
    db: State<'_, Arc<BrainDb>>,
    period_days: Option<u32>,
) -> Result<EconomicReport, BrainError> {
    economics::generate_economic_report(&db, period_days.unwrap_or(30)).await
}
