//! Phase Omega Part VIII — Economic Autonomy
//!
//! Tracks revenue events and compute costs to determine whether the brain
//! is self-sustaining. Revenue sources include projects, services, brain-initiated
//! actions, and manual entries. Costs cover electricity, API calls, and
//! hardware depreciation. An LLM-based value attribution function
//! estimates the economic value the brain generates through its actions.
//!
//! ## Architecture
//!
//! - `RevenueEvent` — a logged revenue event with source and amount
//! - `ComputeCost` — a logged cost event (API, electricity, etc.)
//! - `EconomicReport` — aggregated revenue vs cost analysis
//! - `record_revenue()` — log a revenue event
//! - `record_cost()` — log a compute cost
//! - `generate_economic_report()` — aggregate and calculate ROI
//! - `is_self_sustaining()` — revenue > costs over the last 30 days
//! - `attribute_value()` — LLM-estimated value of brain actions

use crate::db::BrainDb;
use crate::error::BrainError;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

// =========================================================================
// DATA STRUCTURES
// =========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevenueEvent {
    pub id: String,
    pub source: String,           // "project_a", "project_b", "brain_action", "manual"
    pub amount: f64,
    pub currency: String,
    pub description: String,
    pub attributed_to: Option<String>, // agent or circuit that caused this
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputeCost {
    pub id: String,
    pub cost_type: String, // "electricity", "api_call", "hardware_depreciation"
    pub amount: f64,
    pub description: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EconomicReport {
    pub total_revenue: f64,
    pub total_cost: f64,
    pub net_value: f64,
    pub roi: f64,
    pub revenue_by_source: HashMap<String, f64>,
    pub cost_by_type: HashMap<String, f64>,
    pub self_sustaining: bool,
    pub period_days: u32,
}

// =========================================================================
// SCHEMA INIT
// =========================================================================

/// Create economics tables (idempotent).
pub async fn init_economics(db: &Arc<BrainDb>) -> Result<(), BrainError> {
    db.with_conn(|conn| {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS revenue_events (
                id TEXT PRIMARY KEY,
                source TEXT NOT NULL,
                amount REAL NOT NULL,
                currency TEXT NOT NULL DEFAULT 'ZAR',
                description TEXT NOT NULL DEFAULT '',
                attributed_to TEXT,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS compute_costs (
                id TEXT PRIMARY KEY,
                cost_type TEXT NOT NULL,
                amount REAL NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL
            );"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(())
    }).await
}

// =========================================================================
// CORE FUNCTIONS
// =========================================================================

/// Log a revenue event.
pub async fn record_revenue(
    db: &Arc<BrainDb>,
    source: String,
    amount: f64,
    currency: String,
    description: String,
    attributed_to: Option<String>,
) -> Result<RevenueEvent, BrainError> {
    init_economics(db).await?;

    let id = format!("rev:{}", uuid::Uuid::now_v7());
    let now = chrono::Utc::now().to_rfc3339();

    let event = RevenueEvent {
        id: id.clone(),
        source: source.clone(),
        amount,
        currency: currency.clone(),
        description: description.clone(),
        attributed_to: attributed_to.clone(),
        created_at: now.clone(),
    };

    let e = event.clone();
    db.with_conn(move |conn| {
        conn.execute(
            "INSERT INTO revenue_events (id, source, amount, currency, description, attributed_to, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![e.id, e.source, e.amount, e.currency, e.description, e.attributed_to, e.created_at],
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(())
    }).await?;

    log::info!("Revenue recorded: {} {} from '{}' — {}", amount, currency, source, description);
    Ok(event)
}

/// Log a compute cost.
pub async fn record_cost(
    db: &Arc<BrainDb>,
    cost_type: String,
    amount: f64,
    description: String,
) -> Result<ComputeCost, BrainError> {
    init_economics(db).await?;

    let id = format!("cost:{}", uuid::Uuid::now_v7());
    let now = chrono::Utc::now().to_rfc3339();

    let cost = ComputeCost {
        id: id.clone(),
        cost_type: cost_type.clone(),
        amount,
        description: description.clone(),
        created_at: now.clone(),
    };

    let c = cost.clone();
    db.with_conn(move |conn| {
        conn.execute(
            "INSERT INTO compute_costs (id, cost_type, amount, description, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![c.id, c.cost_type, c.amount, c.description, c.created_at],
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(())
    }).await?;

    log::info!("Cost recorded: {} ZAR for '{}' — {}", amount, cost_type, description);
    Ok(cost)
}

/// Generate an economic report aggregating revenue vs costs over a period.
pub async fn generate_economic_report(
    db: &Arc<BrainDb>,
    period_days: u32,
) -> Result<EconomicReport, BrainError> {
    init_economics(db).await?;

    let cutoff = chrono::Utc::now() - chrono::Duration::days(period_days as i64);
    let cutoff_str = cutoff.to_rfc3339();
    let cutoff_str2 = cutoff_str.clone();

    // Get revenue aggregates
    let revenue_data: (f64, HashMap<String, f64>) = db.with_conn(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT source, SUM(amount) FROM revenue_events WHERE created_at > ?1 GROUP BY source"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map(params![cutoff_str], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
        }).map_err(|e| BrainError::Database(e.to_string()))?;

        let mut total = 0.0;
        let mut by_source = HashMap::new();
        for r in rows {
            if let Ok((source, amount)) = r {
                total += amount;
                by_source.insert(source, amount);
            }
        }
        Ok((total, by_source))
    }).await?;

    // Get cost aggregates
    let cost_data: (f64, HashMap<String, f64>) = db.with_conn(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT cost_type, SUM(amount) FROM compute_costs WHERE created_at > ?1 GROUP BY cost_type"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map(params![cutoff_str2], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
        }).map_err(|e| BrainError::Database(e.to_string()))?;

        let mut total = 0.0;
        let mut by_type = HashMap::new();
        for r in rows {
            if let Ok((cost_type, amount)) = r {
                total += amount;
                by_type.insert(cost_type, amount);
            }
        }
        Ok((total, by_type))
    }).await?;

    let total_revenue = revenue_data.0;
    let total_cost = cost_data.0;
    let net_value = total_revenue - total_cost;
    let roi = if total_cost > 0.0 { (net_value / total_cost) * 100.0 } else { 0.0 };

    Ok(EconomicReport {
        total_revenue,
        total_cost,
        net_value,
        roi,
        revenue_by_source: revenue_data.1,
        cost_by_type: cost_data.1,
        self_sustaining: total_revenue > total_cost && total_revenue > 0.0,
        period_days,
    })
}

/// Check if the brain is self-sustaining (revenue > costs over the last 30 days).
pub async fn is_self_sustaining(db: &Arc<BrainDb>) -> Result<bool, BrainError> {
    let report = generate_economic_report(db, 30).await?;
    Ok(report.self_sustaining)
}

/// Use LLM to estimate the value generated by brain actions over a period.
#[allow(dead_code)]
pub async fn attribute_value(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    init_economics(db).await?;

    // Gather recent brain activity stats
    let cutoff = chrono::Utc::now() - chrono::Duration::days(30);
    let cutoff_str = cutoff.to_rfc3339();

    let stats: serde_json::Value = db.with_conn(move |conn| {
        let nodes_created: u64 = conn.query_row(
            "SELECT COUNT(*) FROM nodes WHERE created_at > ?1 AND synthesized_by_brain = 1",
            params![cutoff_str],
            |row| row.get(0),
        ).unwrap_or(0);

        let circuits_run: u64 = conn.query_row(
            "SELECT COUNT(*) FROM autonomy_circuit_log WHERE started_at > ?1 AND status = 'ok'",
            params![cutoff_str],
            |row| row.get(0),
        ).unwrap_or(0);

        let edges_created: u64 = conn.query_row(
            "SELECT COUNT(*) FROM edges WHERE created_at > ?1",
            params![cutoff_str],
            |row| row.get(0),
        ).unwrap_or(0);

        let research_completed: u64 = conn.query_row(
            "SELECT COUNT(*) FROM research_missions WHERE status = 'completed' AND completed_at > ?1",
            params![cutoff_str],
            |row| row.get(0),
        ).unwrap_or(0);

        Ok(serde_json::json!({
            "nodes_created_by_brain": nodes_created,
            "circuits_run_successfully": circuits_run,
            "edges_created": edges_created,
            "research_missions_completed": research_completed,
        }))
    }).await?;

    // Get current economic report for context
    let report = generate_economic_report(db, 30).await?;

    let llm = crate::commands::ai::get_llm_client_deep(db);
    let prompt = format!(
        "You are an economic analyst for an AI brain system. Estimate the economic value \
         generated by this brain's autonomous actions over the last 30 days.\n\n\
         Brain Activity Stats:\n{}\n\n\
         Current Economics:\n\
         - Total Revenue: {:.2} ZAR\n\
         - Total Costs: {:.2} ZAR\n\
         - Net Value: {:.2} ZAR\n\
         - ROI: {:.1}%\n\n\
         Consider:\n\
         1. Value of knowledge nodes created (research, synthesis, insights)\n\
         2. Time saved through automated research and quality improvement\n\
         3. Cross-domain connections that might lead to novel solutions\n\
         4. Quality improvement of existing knowledge base\n\n\
         Provide a brief assessment (3-5 sentences) with an estimated monetary value in ZAR. \
         Be conservative but realistic.",
        serde_json::to_string_pretty(&stats).unwrap_or_default(),
        report.total_revenue,
        report.total_cost,
        report.net_value,
        report.roi,
    );

    let assessment = llm.generate(&prompt, 400).await
        .unwrap_or_else(|e| format!("Value attribution failed: {}", e));

    log::info!("Economic value attribution: {}", assessment);
    Ok(assessment)
}

// =========================================================================
// CIRCUIT — economic_audit
// =========================================================================

/// Circuit entry point: run a periodic economic audit.
/// Estimates API costs from circuit runs, generates a report, and
/// attributes value to brain-generated actions.
pub async fn circuit_economic_audit(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    init_economics(db).await?;

    // Auto-estimate API costs from recent circuit runs
    let cutoff = chrono::Utc::now() - chrono::Duration::hours(24);
    let cutoff_str = cutoff.to_rfc3339();

    let recent_circuits: u64 = db.with_conn(move |conn| {
        conn.query_row(
            "SELECT COUNT(*) FROM autonomy_circuit_log WHERE started_at > ?1",
            params![cutoff_str],
            |row| row.get(0),
        ).map_err(|e| BrainError::Database(e.to_string()))
    }).await.unwrap_or(0);

    // Estimate API cost: ~R0.05 per circuit run (conservative estimate for LLM calls)
    if recent_circuits > 0 {
        let estimated_api_cost = recent_circuits as f64 * 0.05;
        let _ = record_cost(
            db,
            "api_call".to_string(),
            estimated_api_cost,
            format!("Estimated API cost for {} circuit runs (auto)", recent_circuits),
        ).await;
    }

    // Generate the 30-day report
    let report = generate_economic_report(db, 30).await?;

    Ok(format!(
        "Economic audit: revenue={:.2}, cost={:.2}, net={:.2}, ROI={:.1}%, self_sustaining={}",
        report.total_revenue,
        report.total_cost,
        report.net_value,
        report.roi,
        report.self_sustaining
    ))
}
