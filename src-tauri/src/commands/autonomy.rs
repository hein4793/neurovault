use crate::commands::settings::load_settings;
use crate::db::BrainDb;
use crate::error::BrainError;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutonomyTask {
    pub name: String,
    pub last_run: Option<String>,
    pub last_result: Option<String>,
    pub status: String,
    pub runs_today: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutonomyTodayStats {
    pub topics_researched: u32,
    pub nodes_created: u32,
    pub links_made: u32,
    pub quality_improved: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutonomyStatus {
    pub enabled: bool,
    pub tasks: Vec<AutonomyTask>,
    pub today: AutonomyTodayStats,
}

#[derive(Debug, Clone, Deserialize)]
struct TaskStateRow {
    task_name: String,
    last_run_at: String,
    last_result: String,
    runs_today: u32,
    today_date: String,
}

#[tauri::command]
pub async fn get_autonomy_status(
    db: State<'_, Arc<BrainDb>>,
) -> Result<AutonomyStatus, BrainError> {
    let settings = load_settings(&db);
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

    let states: Vec<TaskStateRow> = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT task_name, last_run_at, last_result, runs_today, today_date FROM autonomy_state"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok(TaskStateRow {
                task_name: row.get(0)?,
                last_run_at: row.get(1)?,
                last_result: row.get(2)?,
                runs_today: row.get::<_, i32>(3)? as u32,
                today_date: row.get(4)?,
            })
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(s) = r { result.push(s); } }
        Ok(result)
    }).await?;

    let task_names = [
        "auto_link", "quality_recalc", "quality_sweep",
        "iq_boost", "active_learning", "export",
    ];

    let mut tasks = Vec::new();
    for name in &task_names {
        let state = states.iter().find(|s| s.task_name == *name);
        tasks.push(AutonomyTask {
            name: name.to_string(),
            last_run: state.map(|s| s.last_run_at.clone()),
            last_result: state.map(|s| s.last_result.clone()),
            status: if state.is_some() { "idle".to_string() } else { "pending".to_string() },
            runs_today: state
                .filter(|s| s.today_date == today)
                .map(|s| s.runs_today)
                .unwrap_or(0),
        });
    }

    // Aggregate today stats from task states
    let learning_state = states.iter().find(|s| s.task_name == "active_learning" && s.today_date == today);
    let link_state = states.iter().find(|s| s.task_name == "auto_link" && s.today_date == today);

    // Parse last_result strings for rough counts
    let topics_researched = learning_state.map(|s| s.runs_today).unwrap_or(0);
    let links_made = link_state.map(|s| s.runs_today).unwrap_or(0);

    Ok(AutonomyStatus {
        enabled: settings.autonomy_enabled,
        tasks,
        today: AutonomyTodayStats {
            topics_researched,
            nodes_created: 0, // Would need separate tracking
            links_made,
            quality_improved: 0,
        },
    })
}

#[tauri::command]
pub async fn set_autonomy_enabled(
    db: State<'_, Arc<BrainDb>>,
    enabled: bool,
) -> Result<(), BrainError> {
    let mut settings = load_settings(&db);
    settings.autonomy_enabled = enabled;

    let path = db.config.data_dir.join("settings.json");
    let data = serde_json::to_string_pretty(&settings)
        .map_err(BrainError::Serialization)?;
    std::fs::write(&path, data).map_err(BrainError::Io)?;

    log::info!("Autonomy {}", if enabled { "enabled" } else { "disabled" });
    Ok(())
}

#[tauri::command]
pub async fn trigger_autonomy_task(
    db: State<'_, Arc<BrainDb>>,
    task: String,
) -> Result<String, BrainError> {
    match task.as_str() {
        "auto_link" => {
            let result = db.auto_link_nodes().await?;
            Ok(format!("Created {} synapses", result.created))
        }
        "quality_recalc" => {
            let (q, _) = crate::quality::scoring::calculate_quality_scores(&db).await?;
            let (d, _) = crate::quality::decay::calculate_decay_scores(&db).await?;
            Ok(format!("{} quality + {} decay updated", q, d))
        }
        "export" => {
            crate::export::run_full_export(&db).await?;
            Ok("Full export completed".to_string())
        }
        _ => Err(BrainError::NotFound(format!("Unknown task: {}", task))),
    }
}
