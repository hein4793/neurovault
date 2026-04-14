//! Multi-brain management — Phase 4.1 of the master plan.
//!
//! Lets the user create separate "brains" within one SQLite database.
//! Useful for keeping work and personal knowledge separate without
//! running two copies of the desktop app.
//!
//! ## Model
//!
//! - The "main" brain always exists and contains every node with
//!   `brain_id = None` (back-compat for the existing 800K nodes).
//! - Additional brains are rows in the `brains` table with a unique
//!   slug (e.g. "work", "personal", "research").
//! - The `active_brain_state` singleton row tracks which brain the user
//!   is currently viewing.
//! - When the user creates new content, the brain stamps `brain_id` with
//!   the active slug. When `active_brain_slug == "main"`, brain_id stays
//!   None for back-compat.
//!
//! ## What this module ships
//!
//! - Tauri commands: `list_brains`, `create_brain`, `delete_brain`,
//!   `get_active_brain`, `set_active_brain`, `get_brain_stats_for`
//! - Defaulting + idempotent setup of the "main" brain
//!
//! ## What it does NOT ship (deferred)
//!
//! - Per-brain HNSW indexes (today HNSW is global). At brain scale <= 5
//!   brains the cross-brain pollution in semantic search is acceptable.
//! - Per-brain settings (every brain currently shares Ollama URL, models,
//!   autonomy intervals).

use crate::db::models::{Brain, MAIN_BRAIN};
use crate::db::BrainDb;
use crate::error::BrainError;
use rusqlite::params;
use std::sync::Arc;
use tauri::State;

/// List every registered brain. Always includes "main" — synthesizes it
/// if there's no row for it yet.
#[tauri::command]
pub async fn list_brains(db: State<'_, Arc<BrainDb>>) -> Result<Vec<Brain>, BrainError> {
    let mut rows: Vec<Brain> = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, slug, name, description, color, created_at FROM brains ORDER BY slug ASC"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let mapped = stmt.query_map([], |row| {
            Ok(Brain {
                id: row.get(0)?,
                slug: row.get(1)?,
                name: row.get(2)?,
                description: row.get(3)?,
                color: row.get(4)?,
                created_at: row.get(5)?,
            })
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in mapped { if let Ok(b) = r { result.push(b); } }
        Ok(result)
    }).await?;

    let has_main = rows.iter().any(|b| b.slug == MAIN_BRAIN);
    if !has_main {
        // Synthesize the main brain entry so the picker always shows it.
        // We don't insert it — that's done lazily on the first explicit
        // create_brain call.
        rows.insert(
            0,
            Brain {
                id: None,
                slug: MAIN_BRAIN.to_string(),
                name: "Main Brain".to_string(),
                description: "All knowledge with no explicit brain_id".to_string(),
                color: "#00A8FF".to_string(),
                created_at: "1970-01-01T00:00:00Z".to_string(),
            },
        );
    }
    Ok(rows)
}

/// Create a new brain. Slug must be unique and lowercase.
#[tauri::command]
pub async fn create_brain(
    db: State<'_, Arc<BrainDb>>,
    slug: String,
    name: String,
    description: Option<String>,
    color: Option<String>,
) -> Result<Brain, BrainError> {
    let slug = slug.trim().to_lowercase();
    if slug.is_empty() || slug.len() > 30 {
        return Err(BrainError::Internal("slug must be 1-30 chars".into()));
    }
    if !slug.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return Err(BrainError::Internal(
            "slug must be alphanumeric, underscore, or hyphen".into(),
        ));
    }
    if slug == MAIN_BRAIN {
        return Err(BrainError::Internal(format!("brain '{}' already exists", slug)));
    }

    let now = chrono::Utc::now().to_rfc3339();
    let id = format!("brain:{}", uuid::Uuid::now_v7());
    let brain = Brain {
        id: Some(id.clone()),
        slug: slug.clone(),
        name: if name.is_empty() { slug.clone() } else { name },
        description: description.unwrap_or_default(),
        color: color.unwrap_or_else(|| "#8B5CF6".to_string()),
        created_at: now,
    };

    let brain_clone = brain.clone();
    db.with_conn(move |conn| {
        conn.execute(
            "INSERT INTO brains (id, slug, name, description, color, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                brain_clone.id,
                brain_clone.slug,
                brain_clone.name,
                brain_clone.description,
                brain_clone.color,
                brain_clone.created_at,
            ],
        ).map_err(|e| BrainError::Database(e.to_string()))
    }).await?;

    log::info!("Created brain: {}", brain.slug);
    Ok(brain)
}

/// Delete a brain. Refuses to delete "main". Does NOT delete the brain's
/// nodes — that's a separate explicit operation. The brain entry is
/// removed and any nodes that had `brain_id = $slug` become orphaned
/// (still queryable, just not associated with a registered brain).
#[tauri::command]
pub async fn delete_brain(
    db: State<'_, Arc<BrainDb>>,
    slug: String,
) -> Result<bool, BrainError> {
    if slug == MAIN_BRAIN {
        return Err(BrainError::Internal("cannot delete the main brain".into()));
    }
    let slug_clone = slug.clone();
    db.with_conn(move |conn| {
        conn.execute("DELETE FROM brains WHERE slug = ?1", params![slug_clone])
            .map_err(|e| BrainError::Database(e.to_string()))
    }).await?;
    log::info!("Deleted brain: {}", slug);
    Ok(true)
}

/// Get the currently active brain slug. Defaults to "main" if not set.
#[tauri::command]
pub async fn get_active_brain(db: State<'_, Arc<BrainDb>>) -> Result<String, BrainError> {
    let slug: Option<String> = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT active_brain_slug FROM active_brain_state LIMIT 1"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut rows = stmt.query_map([], |row| {
            row.get::<_, String>(0)
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        match rows.next() {
            Some(Ok(s)) => Ok(Some(s)),
            _ => Ok(None),
        }
    }).await?;
    Ok(slug.unwrap_or_else(|| MAIN_BRAIN.to_string()))
}

/// Switch the active brain. Persisted in the singleton `active_brain_state` row.
#[tauri::command]
pub async fn set_active_brain(
    db: State<'_, Arc<BrainDb>>,
    slug: String,
) -> Result<String, BrainError> {
    let slug = slug.trim().to_lowercase();
    let now = chrono::Utc::now().to_rfc3339();
    let slug_clone = slug.clone();
    db.with_conn(move |conn| {
        conn.execute("DELETE FROM active_brain_state", [])
            .map_err(|e| BrainError::Database(e.to_string()))?;
        conn.execute(
            "INSERT INTO active_brain_state (id, active_brain_slug, updated_at) \
             VALUES ('current', ?1, ?2)",
            params![slug_clone, now],
        ).map_err(|e| BrainError::Database(e.to_string()))
    }).await?;
    log::info!("Active brain switched to: {}", slug);
    Ok(slug)
}

/// Get node count + edge count + IQ for a specific brain.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BrainStatsRow {
    pub slug: String,
    pub total_nodes: u64,
    pub total_edges: u64,
}

#[tauri::command]
pub async fn get_brain_stats_for(
    db: State<'_, Arc<BrainDb>>,
    slug: String,
) -> Result<BrainStatsRow, BrainError> {
    let is_main = slug == MAIN_BRAIN;
    let slug_clone = slug.clone();

    let (node_count, edge_count) = db.with_conn(move |conn| -> Result<(u64, u64), BrainError> {
        let node_count: i64 = if is_main {
            conn.query_row(
                "SELECT COUNT(*) FROM nodes WHERE brain_id IS NULL OR brain_id = 'main'",
                [],
                |row| row.get(0),
            ).unwrap_or(0)
        } else {
            conn.query_row(
                "SELECT COUNT(*) FROM nodes WHERE brain_id = ?1",
                params![slug_clone],
                |row| row.get(0),
            ).unwrap_or(0)
        };

        let edge_count: i64 = if is_main {
            conn.query_row("SELECT COUNT(*) FROM edges", [], |row| row.get(0))
                .unwrap_or(0)
        } else {
            0 // Edges aren't directly scoped to a brain
        };

        Ok((node_count as u64, edge_count as u64))
    }).await?;

    Ok(BrainStatsRow {
        slug,
        total_nodes: node_count,
        total_edges: edge_count,
    })
}
