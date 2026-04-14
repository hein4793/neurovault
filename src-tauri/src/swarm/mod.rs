//! Phase Omega Part II — Agent Swarm Orchestrator.
//!
//! A multi-agent system that decomposes high-level goals into concrete
//! tasks, assigns them to specialised agents, and executes them via the
//! brain's LLM pipeline. Each agent has a distinct system prompt and
//! capability set. The orchestrator runs on a 5-minute loop, checking
//! for actionable tasks, executing them, and propagating results.
//!
//! ## Agents (pre-seeded on first run)
//!
//! | Name       | Speciality                                    |
//! |------------|-----------------------------------------------|
//! | coder      | Code writing, debugging, architecture         |
//! | analyst    | Data analysis, reports, metrics                |
//! | researcher | Information gathering, documentation           |
//! | planner    | Strategic planning, roadmaps, prioritisation   |
//! | auditor    | Code review, quality checks, security          |

use crate::commands::ai::get_llm_client_deep;
use crate::db::BrainDb;
use crate::error::BrainError;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// =========================================================================
// Data structures
// =========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSpec {
    pub id: String,
    pub name: String,
    pub system_prompt: String,
    pub capabilities: Vec<String>,
    pub autonomy_level: f32,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmTask {
    pub id: String,
    pub title: String,
    pub description: String,
    pub assigned_agent: Option<String>,
    pub status: String,
    pub priority: f32,
    pub dependencies: Vec<String>,
    pub result: Option<String>,
    pub parent_goal: Option<String>,
    pub created_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    pub id: String,
    pub from_agent: String,
    pub to_agent: String,
    pub message_type: String,
    pub content: String,
    pub priority: u8,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmStatus {
    pub agent_count: usize,
    pub agents: Vec<AgentSpec>,
    pub total_tasks: usize,
    pub pending_tasks: usize,
    pub in_progress_tasks: usize,
    pub completed_tasks: usize,
    pub failed_tasks: usize,
    pub recent_messages: Vec<AgentMessage>,
}

// =========================================================================
// Default agents
// =========================================================================

const DEFAULT_AGENTS: &[(&str, &str, &[&str], f32)] = &[
    (
        "coder",
        "You are a senior software engineer agent inside a personal knowledge brain. \
         Your speciality is writing, debugging, and architecting code. You produce \
         clean, well-documented, production-grade code. When given a coding task, \
         output the code or solution directly. Be precise and technical.",
        &["code_write", "code_debug", "code_review", "architecture", "refactor"],
        0.7,
    ),
    (
        "analyst",
        "You are a data analyst agent inside a personal knowledge brain. \
         Your speciality is analysing data, generating reports, computing metrics, \
         and identifying trends. When given an analysis task, produce structured \
         findings with numbers and actionable conclusions.",
        &["data_analysis", "report_generation", "metrics", "trend_detection", "visualization"],
        0.6,
    ),
    (
        "researcher",
        "You are a research agent inside a personal knowledge brain. \
         Your speciality is gathering information, reading documentation, \
         summarising sources, and producing comprehensive research briefs. \
         When given a research task, be thorough and cite specifics.",
        &["information_gathering", "documentation", "summarization", "literature_review", "fact_checking"],
        0.8,
    ),
    (
        "planner",
        "You are a strategic planning agent inside a personal knowledge brain. \
         Your speciality is breaking down goals into steps, creating roadmaps, \
         prioritising work, and designing project plans. When given a planning \
         task, produce actionable, time-bound plans with clear milestones.",
        &["strategic_planning", "roadmap", "prioritization", "project_management", "goal_decomposition"],
        0.5,
    ),
    (
        "auditor",
        "You are a quality auditor agent inside a personal knowledge brain. \
         Your speciality is reviewing code for bugs and security issues, \
         checking quality standards, and validating correctness. When given \
         an audit task, be thorough and flag every issue with severity levels.",
        &["code_review", "security_audit", "quality_check", "compliance", "testing"],
        0.4,
    ),
];

// =========================================================================
// Schema + seeding
// =========================================================================

/// Ensure swarm tables exist and seed default agents on first run.
pub async fn init_swarm(db: &Arc<BrainDb>) -> Result<(), BrainError> {
    // Create tables
    db.with_conn(|conn| {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS swarm_agents (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                system_prompt TEXT NOT NULL DEFAULT '',
                capabilities TEXT NOT NULL DEFAULT '[]',
                autonomy_level REAL NOT NULL DEFAULT 0.5,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS swarm_tasks (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                assigned_agent TEXT,
                status TEXT NOT NULL DEFAULT 'pending',
                priority REAL NOT NULL DEFAULT 0.5,
                dependencies TEXT NOT NULL DEFAULT '[]',
                result TEXT,
                parent_goal TEXT,
                created_at TEXT NOT NULL,
                completed_at TEXT
            );

            CREATE TABLE IF NOT EXISTS swarm_messages (
                id TEXT PRIMARY KEY,
                from_agent TEXT NOT NULL,
                to_agent TEXT NOT NULL,
                message_type TEXT NOT NULL,
                content TEXT NOT NULL,
                priority INTEGER NOT NULL DEFAULT 5,
                created_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_swarm_tasks_status ON swarm_tasks(status);
            CREATE INDEX IF NOT EXISTS idx_swarm_tasks_agent ON swarm_tasks(assigned_agent);
            CREATE INDEX IF NOT EXISTS idx_swarm_tasks_parent ON swarm_tasks(parent_goal);
            CREATE INDEX IF NOT EXISTS idx_swarm_messages_to ON swarm_messages(to_agent);
            CREATE INDEX IF NOT EXISTS idx_swarm_messages_from ON swarm_messages(from_agent);
            "
        ).map_err(|e| BrainError::Database(format!("Swarm schema init failed: {}", e)))?;
        Ok(())
    }).await?;

    // Seed default agents if the table is empty
    let count: u64 = db.with_conn(|conn| {
        conn.query_row("SELECT COUNT(*) FROM swarm_agents", [], |row| row.get(0))
            .map_err(|e| BrainError::Database(e.to_string()))
    }).await?;

    if count == 0 {
        log::info!("Swarm: seeding {} default agents", DEFAULT_AGENTS.len());
        for (name, system_prompt, capabilities, autonomy_level) in DEFAULT_AGENTS {
            let id = format!("swarm_agent:{}", uuid::Uuid::now_v7());
            let now = chrono::Utc::now().to_rfc3339();
            let caps_json = serde_json::to_string(&capabilities.to_vec())
                .unwrap_or_else(|_| "[]".to_string());
            let name = name.to_string();
            let prompt = system_prompt.to_string();
            let al = *autonomy_level;
            db.with_conn(move |conn| {
                conn.execute(
                    "INSERT INTO swarm_agents (id, name, system_prompt, capabilities, autonomy_level, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![id, name, prompt, caps_json, al, now],
                ).map_err(|e| BrainError::Database(e.to_string()))?;
                Ok(())
            }).await?;
        }
        log::info!("Swarm: default agents seeded");
    }

    Ok(())
}

// =========================================================================
// Query helpers
// =========================================================================

/// Load all agents from the database.
pub async fn get_agents(db: &Arc<BrainDb>) -> Result<Vec<AgentSpec>, BrainError> {
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, name, system_prompt, capabilities, autonomy_level, created_at
             FROM swarm_agents ORDER BY name"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok(AgentSpec {
                id: row.get(0)?,
                name: row.get(1)?,
                system_prompt: row.get(2)?,
                capabilities: serde_json::from_str(&row.get::<_, String>(3)?).unwrap_or_default(),
                autonomy_level: row.get(4)?,
                created_at: row.get(5)?,
            })
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(a) = r { result.push(a); } }
        Ok(result)
    }).await
}

/// Load all tasks from the database (most recent first).
pub async fn get_tasks(db: &Arc<BrainDb>) -> Result<Vec<SwarmTask>, BrainError> {
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, title, description, assigned_agent, status, priority,
                    dependencies, result, parent_goal, created_at, completed_at
             FROM swarm_tasks ORDER BY created_at DESC LIMIT 200"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok(SwarmTask {
                id: row.get(0)?,
                title: row.get(1)?,
                description: row.get(2)?,
                assigned_agent: row.get(3)?,
                status: row.get(4)?,
                priority: row.get(5)?,
                dependencies: serde_json::from_str(&row.get::<_, String>(6)?).unwrap_or_default(),
                result: row.get(7)?,
                parent_goal: row.get(8)?,
                created_at: row.get(9)?,
                completed_at: row.get(10)?,
            })
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(t) = r { result.push(t); } }
        Ok(result)
    }).await
}

/// Load recent messages.
fn get_recent_messages_sync(conn: &rusqlite::Connection) -> Vec<AgentMessage> {
    let mut stmt = match conn.prepare(
        "SELECT id, from_agent, to_agent, message_type, content, priority, created_at
         FROM swarm_messages ORDER BY created_at DESC LIMIT 50"
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let rows = match stmt.query_map([], |row| {
        Ok(AgentMessage {
            id: row.get(0)?,
            from_agent: row.get(1)?,
            to_agent: row.get(2)?,
            message_type: row.get(3)?,
            content: row.get(4)?,
            priority: row.get::<_, i32>(5)? as u8,
            created_at: row.get(6)?,
        })
    }) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    let mut result = Vec::new();
    for r in rows { if let Ok(m) = r { result.push(m); } }
    result
}

/// Get full swarm status.
pub async fn get_swarm_status_inner(db: &Arc<BrainDb>) -> Result<SwarmStatus, BrainError> {
    let agents = get_agents(db).await?;

    db.with_conn(move |conn| {
        let total: usize = conn.query_row(
            "SELECT COUNT(*) FROM swarm_tasks", [], |row| row.get(0)
        ).unwrap_or(0);
        let pending: usize = conn.query_row(
            "SELECT COUNT(*) FROM swarm_tasks WHERE status = 'pending'", [], |row| row.get(0)
        ).unwrap_or(0);
        let in_progress: usize = conn.query_row(
            "SELECT COUNT(*) FROM swarm_tasks WHERE status IN ('assigned', 'in_progress')", [], |row| row.get(0)
        ).unwrap_or(0);
        let completed: usize = conn.query_row(
            "SELECT COUNT(*) FROM swarm_tasks WHERE status = 'completed'", [], |row| row.get(0)
        ).unwrap_or(0);
        let failed: usize = conn.query_row(
            "SELECT COUNT(*) FROM swarm_tasks WHERE status = 'failed'", [], |row| row.get(0)
        ).unwrap_or(0);

        let recent_messages = get_recent_messages_sync(conn);

        Ok(SwarmStatus {
            agent_count: agents.len(),
            agents,
            total_tasks: total,
            pending_tasks: pending,
            in_progress_tasks: in_progress,
            completed_tasks: completed,
            failed_tasks: failed,
            recent_messages,
        })
    }).await
}

// =========================================================================
// Task creation
// =========================================================================

/// Create a single swarm task.
pub async fn create_task(
    db: &Arc<BrainDb>,
    title: String,
    description: String,
    priority: f32,
    dependencies: Vec<String>,
    parent_goal: Option<String>,
) -> Result<SwarmTask, BrainError> {
    let id = format!("swarm_task:{}", uuid::Uuid::now_v7());
    let now = chrono::Utc::now().to_rfc3339();
    let deps_json = serde_json::to_string(&dependencies).unwrap_or_else(|_| "[]".to_string());

    let task = SwarmTask {
        id: id.clone(),
        title: title.clone(),
        description: description.clone(),
        assigned_agent: None,
        status: "pending".to_string(),
        priority,
        dependencies: dependencies.clone(),
        result: None,
        parent_goal: parent_goal.clone(),
        created_at: now.clone(),
        completed_at: None,
    };

    db.with_conn(move |conn| {
        conn.execute(
            "INSERT INTO swarm_tasks (id, title, description, assigned_agent, status, priority,
                                      dependencies, result, parent_goal, created_at, completed_at)
             VALUES (?1, ?2, ?3, NULL, 'pending', ?4, ?5, NULL, ?6, ?7, NULL)",
            params![id, title, description, priority, deps_json, parent_goal, now],
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(())
    }).await?;

    Ok(task)
}

/// Send a message between agents.
async fn send_message(
    db: &Arc<BrainDb>,
    from: &str,
    to: &str,
    msg_type: &str,
    content: &str,
    priority: u8,
) -> Result<(), BrainError> {
    let id = format!("swarm_msg:{}", uuid::Uuid::now_v7());
    let now = chrono::Utc::now().to_rfc3339();
    let from = from.to_string();
    let to = to.to_string();
    let msg_type = msg_type.to_string();
    let content = content.to_string();
    db.with_conn(move |conn| {
        conn.execute(
            "INSERT INTO swarm_messages (id, from_agent, to_agent, message_type, content, priority, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, from, to, msg_type, content, priority as i32, now],
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(())
    }).await
}

// =========================================================================
// Task value scoring
// =========================================================================

/// Score a task for execution priority. Higher = do first.
///
/// Weights:
///   business_impact * 0.30 + urgency * 0.25 + dependency_unlock * 0.20
///   + fingerprint_fit * 0.15 + effort_efficiency * 0.10
async fn score_task(task: &SwarmTask, db: &Arc<BrainDb>) -> f32 {
    let business_impact = task.priority; // 0.0 .. 1.0

    // Urgency: tasks created longer ago are more urgent (capped at 1.0)
    let urgency = if let Ok(created) = chrono::DateTime::parse_from_rfc3339(&task.created_at) {
        let age_hours = (chrono::Utc::now() - created.with_timezone(&chrono::Utc))
            .num_hours()
            .max(0) as f32;
        (age_hours / 48.0).min(1.0) // fully urgent after 48h
    } else {
        0.5
    };

    // Dependency unlock: how many other tasks depend on this one?
    let task_id = task.id.clone();
    let unlock_count: usize = db.with_conn(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT dependencies FROM swarm_tasks WHERE status = 'pending'"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            let deps_str: String = row.get(0)?;
            let deps: Vec<String> = serde_json::from_str(&deps_str).unwrap_or_default();
            Ok(deps)
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut count = 0;
        for r in rows {
            if let Ok(deps) = r {
                if deps.contains(&task_id) {
                    count += 1;
                }
            }
        }
        Ok(count)
    }).await.unwrap_or(0);
    let dependency_unlock = (unlock_count as f32 / 5.0).min(1.0);

    // Fingerprint fit: does the assigned agent match the task topic?
    // Simple heuristic — if an agent is already assigned, assume fit is good.
    let fingerprint_fit = if task.assigned_agent.is_some() { 0.8 } else { 0.5 };

    // Effort efficiency: shorter titles/descriptions suggest simpler tasks
    let effort_efficiency = if task.description.len() < 200 { 0.8 } else { 0.5 };

    business_impact * 0.30
        + urgency * 0.25
        + dependency_unlock * 0.20
        + fingerprint_fit * 0.15
        + effort_efficiency * 0.10
}

// =========================================================================
// Goal decomposition
// =========================================================================

/// Decompose a high-level goal into 5-15 concrete tasks using the DEEP LLM.
/// Assigns each task to the best agent and creates dependency chains.
pub async fn decompose_goal(db: &Arc<BrainDb>, goal: &str) -> Result<Vec<SwarmTask>, BrainError> {
    let agents = get_agents(db).await?;
    if agents.is_empty() {
        return Err(BrainError::Internal("No swarm agents available. Run init_swarm first.".into()));
    }

    // Build agent descriptions for the LLM
    let agent_list: String = agents.iter()
        .map(|a| format!("- {} (capabilities: {})", a.name, a.capabilities.join(", ")))
        .collect::<Vec<_>>()
        .join("\n");

    // Gather brain context for the goal
    let goal_str = goal.to_string();
    let related = {
        let client = crate::embeddings::OllamaClient::new(
            db.config.ollama_url.clone(),
            db.config.embedding_model.clone(),
        );
        if client.health_check().await {
            match client.generate_embedding(&goal_str).await {
                Ok(emb) => db.vector_search(emb, 5).await.unwrap_or_default(),
                Err(_) => db.search_nodes(&goal_str).await.unwrap_or_default(),
            }
        } else {
            db.search_nodes(&goal_str).await.unwrap_or_default()
        }
    };

    let context: String = if related.is_empty() {
        String::new()
    } else {
        let mut ctx = String::from("RELEVANT BRAIN KNOWLEDGE:\n");
        for r in related.iter().take(5) {
            ctx.push_str(&format!("- {}: {}\n", r.node.title, r.node.summary));
        }
        ctx.push('\n');
        ctx
    };

    let llm = get_llm_client_deep(db);
    let prompt = format!(
        r#"You are the swarm orchestrator for a personal knowledge brain. Decompose the goal below into 5-15 concrete, actionable tasks. For each task, specify:
1. A short title (max 80 chars)
2. A description of exactly what to do
3. Which agent should handle it (pick from the list below)
4. A priority from 0.0 to 1.0
5. Which task numbers it depends on (if any)

AVAILABLE AGENTS:
{agent_list}

{context}GOAL: {goal}

Respond ONLY with a valid JSON array. Each element must have these exact keys:
  "title": string,
  "description": string,
  "agent": string (one of: {agent_names}),
  "priority": number,
  "depends_on": array of integers (1-based task indices, empty if none)

Example:
[
  {{"title":"Research existing patterns","description":"Gather information about...","agent":"researcher","priority":0.9,"depends_on":[]}},
  {{"title":"Implement core module","description":"Write the code for...","agent":"coder","priority":0.8,"depends_on":[1]}}
]"#,
        agent_list = agent_list,
        context = context,
        goal = goal,
        agent_names = agents.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", "),
    );

    let raw = llm.generate(&prompt, 2000).await?;

    // Parse the JSON response — try to extract a JSON array even if there's
    // surrounding text.
    let json_str = extract_json_array(&raw)
        .ok_or_else(|| BrainError::Internal(format!(
            "LLM did not return valid JSON array for goal decomposition. Raw: {}",
            crate::truncate_str(&raw, 500)
        )))?;

    let parsed: Vec<serde_json::Value> = serde_json::from_str(&json_str)
        .map_err(|e| BrainError::Internal(format!("Failed to parse decomposed tasks: {}", e)))?;

    if parsed.is_empty() {
        return Err(BrainError::Internal("LLM returned empty task list".into()));
    }

    let goal_id = format!("swarm_goal:{}", uuid::Uuid::now_v7());

    // First pass: create all tasks and record their IDs
    let mut task_ids: Vec<String> = Vec::new();
    let mut tasks: Vec<SwarmTask> = Vec::new();

    for item in &parsed {
        let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("Untitled task").to_string();
        let description = item.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let priority = item.get("priority").and_then(|v| v.as_f64()).unwrap_or(0.5) as f32;
        let agent_name = item.get("agent").and_then(|v| v.as_str()).unwrap_or("").to_string();

        let task = create_task(db, title, description, priority, vec![], Some(goal_id.clone())).await?;
        task_ids.push(task.id.clone());
        tasks.push(task);

        // Assign the agent if it exists
        if !agent_name.is_empty() {
            if let Some(agent) = agents.iter().find(|a| a.name == agent_name) {
                let task_id = task_ids.last().unwrap().clone();
                let agent_id = agent.id.clone();
                db.with_conn(move |conn| {
                    conn.execute(
                        "UPDATE swarm_tasks SET assigned_agent = ?1 WHERE id = ?2",
                        params![agent_id, task_id],
                    ).map_err(|e| BrainError::Database(e.to_string()))?;
                    Ok(())
                }).await?;

                // Update in-memory task
                if let Some(t) = tasks.last_mut() {
                    t.assigned_agent = Some(agent.id.clone());
                }
            }
        }
    }

    // Second pass: wire up dependency chains
    for (idx, item) in parsed.iter().enumerate() {
        let depends_on = item.get("depends_on")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        if depends_on.is_empty() { continue; }

        let dep_ids: Vec<String> = depends_on.iter()
            .filter_map(|v| v.as_u64())
            .filter_map(|i| {
                let idx = (i as usize).checked_sub(1)?;
                task_ids.get(idx).cloned()
            })
            .collect();

        if dep_ids.is_empty() { continue; }

        let deps_json = serde_json::to_string(&dep_ids).unwrap_or_else(|_| "[]".to_string());
        let task_id = task_ids[idx].clone();
        db.with_conn(move |conn| {
            conn.execute(
                "UPDATE swarm_tasks SET dependencies = ?1 WHERE id = ?2",
                params![deps_json, task_id],
            ).map_err(|e| BrainError::Database(e.to_string()))?;
            Ok(())
        }).await?;

        // Update in-memory
        if let Some(t) = tasks.get_mut(idx) {
            t.dependencies = dep_ids;
        }
    }

    // Log goal decomposition message
    let _ = send_message(
        db,
        "orchestrator",
        "all",
        "inform",
        &format!("Goal decomposed into {} tasks: {}", tasks.len(), goal),
        8,
    ).await;

    log::info!(
        "Swarm: decomposed goal '{}' into {} tasks (goal_id={})",
        crate::truncate_str(goal, 100), tasks.len(), goal_id
    );

    Ok(tasks)
}

/// Extract a JSON array from text that may contain surrounding prose.
fn extract_json_array(text: &str) -> Option<String> {
    // Try the raw text first
    if serde_json::from_str::<Vec<serde_json::Value>>(text.trim()).is_ok() {
        return Some(text.trim().to_string());
    }
    // Find the first '[' and last ']'
    let start = text.find('[')?;
    let end = text.rfind(']')?;
    if end <= start { return None; }
    let slice = &text[start..=end];
    if serde_json::from_str::<Vec<serde_json::Value>>(slice).is_ok() {
        Some(slice.to_string())
    } else {
        None
    }
}

// =========================================================================
// Orchestrator loop
// =========================================================================

/// Main orchestrator loop. Runs every 5 minutes:
/// 1. Check for pending tasks with satisfied dependencies
/// 2. Assign to appropriate agent
/// 3. Execute task (LLM call with agent system prompt + task + brain context)
/// 4. Store result
/// 5. Check if any goals are fully complete
pub async fn run_swarm_orchestrator(db: Arc<BrainDb>) {
    log::info!("Swarm orchestrator starting...");

    // Wait for other systems to initialise
    tokio::time::sleep(std::time::Duration::from_secs(60)).await;

    // Initialise schema + seed agents
    if let Err(e) = init_swarm(&db).await {
        log::error!("Swarm: failed to init — {}", e);
        return;
    }

    log::info!("Swarm orchestrator initialised — entering 5-minute loop");

    loop {
        if let Err(e) = orchestrator_tick(&db).await {
            log::warn!("Swarm orchestrator tick failed: {}", e);
        }
        tokio::time::sleep(std::time::Duration::from_secs(300)).await;
    }
}

/// Single orchestrator tick — process one batch of ready tasks.
async fn orchestrator_tick(db: &Arc<BrainDb>) -> Result<(), BrainError> {
    // 1. Find pending tasks whose dependencies are all completed
    let ready_tasks = find_ready_tasks(db).await?;

    if ready_tasks.is_empty() {
        log::debug!("Swarm: no ready tasks this tick");
        return Ok(());
    }

    log::info!("Swarm: {} tasks ready for execution", ready_tasks.len());

    // 2. Score and sort tasks
    let mut scored: Vec<(SwarmTask, f32)> = Vec::new();
    for task in ready_tasks {
        let score = score_task(&task, db).await;
        scored.push((task, score));
    }
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // 3. Execute up to 3 tasks per tick (to avoid monopolising the LLM)
    let agents = get_agents(db).await?;
    for (task, score) in scored.into_iter().take(3) {
        log::info!("Swarm: executing task '{}' (score={:.2})", task.title, score);
        execute_task(db, &task, &agents).await;
    }

    // 4. Check for fully-completed goals
    check_completed_goals(db).await;

    Ok(())
}

/// Find tasks that are pending and have all dependencies satisfied.
async fn find_ready_tasks(db: &Arc<BrainDb>) -> Result<Vec<SwarmTask>, BrainError> {
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, title, description, assigned_agent, status, priority,
                    dependencies, result, parent_goal, created_at, completed_at
             FROM swarm_tasks WHERE status = 'pending' ORDER BY priority DESC"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok(SwarmTask {
                id: row.get(0)?,
                title: row.get(1)?,
                description: row.get(2)?,
                assigned_agent: row.get(3)?,
                status: row.get(4)?,
                priority: row.get(5)?,
                dependencies: serde_json::from_str(&row.get::<_, String>(6)?).unwrap_or_default(),
                result: row.get(7)?,
                parent_goal: row.get(8)?,
                created_at: row.get(9)?,
                completed_at: row.get(10)?,
            })
        }).map_err(|e| BrainError::Database(e.to_string()))?;

        let mut all_tasks = Vec::new();
        for r in rows { if let Ok(t) = r { all_tasks.push(t); } }

        // Filter to tasks whose dependencies are all completed
        let mut ready = Vec::new();
        for task in all_tasks {
            if task.dependencies.is_empty() {
                ready.push(task);
                continue;
            }
            // Check each dependency
            let all_done = task.dependencies.iter().all(|dep_id| {
                let status: String = conn.query_row(
                    "SELECT status FROM swarm_tasks WHERE id = ?1",
                    params![dep_id],
                    |row| row.get(0),
                ).unwrap_or_else(|_| "completed".to_string()); // if dep not found, treat as done
                status == "completed"
            });
            if all_done {
                ready.push(task);
            }
        }

        Ok(ready)
    }).await
}

/// Execute a single task using the assigned (or best-fit) agent.
async fn execute_task(db: &Arc<BrainDb>, task: &SwarmTask, agents: &[AgentSpec]) {
    // Find the agent
    let agent = if let Some(ref agent_id) = task.assigned_agent {
        agents.iter().find(|a| a.id == *agent_id)
    } else {
        // Auto-assign by matching task title keywords to capabilities
        find_best_agent(task, agents)
    };

    let agent = match agent {
        Some(a) => a,
        None => {
            log::warn!("Swarm: no agent found for task '{}', using first available", task.title);
            match agents.first() {
                Some(a) => a,
                None => {
                    mark_task_failed(db, &task.id, "No agents available").await;
                    return;
                }
            }
        }
    };

    // Mark task as in_progress
    let task_id = task.id.clone();
    let agent_id = agent.id.clone();
    let _ = db.with_conn(move |conn| {
        conn.execute(
            "UPDATE swarm_tasks SET status = 'in_progress', assigned_agent = ?1 WHERE id = ?2",
            params![agent_id, task_id],
        ).map_err(|e| BrainError::Database(e.to_string()))
    }).await;

    // Build context from dependency results
    let dep_context = build_dependency_context(db, &task.dependencies).await;

    // Build brain context
    let brain_context = {
        let search_query = format!("{} {}", task.title, task.description);
        let related = db.search_nodes(&search_query).await.unwrap_or_default();
        if related.is_empty() {
            String::new()
        } else {
            let mut ctx = String::from("BRAIN KNOWLEDGE:\n");
            for r in related.iter().take(3) {
                ctx.push_str(&format!("- {}: {}\n", r.node.title, r.node.summary));
            }
            ctx.push('\n');
            ctx
        }
    };

    // Build the prompt
    let prompt = format!(
        "{system_prompt}\n\n{brain_ctx}{dep_ctx}TASK: {title}\n\nDESCRIPTION: {description}\n\n\
         Execute this task completely and provide a detailed result. Be concrete and actionable.",
        system_prompt = agent.system_prompt,
        brain_ctx = brain_context,
        dep_ctx = dep_context,
        title = task.title,
        description = task.description,
    );

    let llm = get_llm_client_deep(db);
    match llm.generate(&prompt, 1500).await {
        Ok(result) => {
            let result_truncated = crate::truncate_str(&result, 4000).to_string();
            mark_task_completed(db, &task.id, &result_truncated).await;

            // Send completion message
            let _ = send_message(
                db,
                &agent.name,
                "orchestrator",
                "complete",
                &format!("Completed: {}", task.title),
                5,
            ).await;

            log::info!("Swarm: task '{}' completed by agent '{}'", task.title, agent.name);
        }
        Err(e) => {
            let err_msg = format!("LLM error: {}", e);
            mark_task_failed(db, &task.id, &err_msg).await;

            let _ = send_message(
                db,
                &agent.name,
                "orchestrator",
                "inform",
                &format!("Failed: {} — {}", task.title, err_msg),
                8,
            ).await;

            log::warn!("Swarm: task '{}' failed — {}", task.title, err_msg);
        }
    }
}

/// Find the best agent for a task based on keyword matching.
fn find_best_agent<'a>(task: &SwarmTask, agents: &'a [AgentSpec]) -> Option<&'a AgentSpec> {
    let text = format!("{} {}", task.title, task.description).to_lowercase();
    let mut best: Option<(&AgentSpec, usize)> = None;

    for agent in agents {
        let hits: usize = agent.capabilities.iter()
            .filter(|cap| {
                cap.split('_').any(|word| text.contains(word))
            })
            .count();

        if hits > 0 {
            if best.is_none() || hits > best.unwrap().1 {
                best = Some((agent, hits));
            }
        }
    }

    best.map(|(a, _)| a)
}

/// Build context string from completed dependency results.
async fn build_dependency_context(db: &Arc<BrainDb>, dep_ids: &[String]) -> String {
    if dep_ids.is_empty() { return String::new(); }

    let ids = dep_ids.to_vec();
    let deps: Vec<(String, String)> = db.with_conn(move |conn| {
        let mut result = Vec::new();
        for id in &ids {
            if let Ok((title, res)) = conn.query_row(
                "SELECT title, COALESCE(result, '') FROM swarm_tasks WHERE id = ?1",
                params![id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            ) {
                result.push((title, res));
            }
        }
        Ok(result)
    }).await.unwrap_or_default();

    if deps.is_empty() { return String::new(); }

    let mut ctx = String::from("RESULTS FROM PREREQUISITE TASKS:\n");
    for (title, result) in &deps {
        let r = crate::truncate_str(result, 500);
        ctx.push_str(&format!("- {}: {}\n", title, r));
    }
    ctx.push('\n');
    ctx
}

/// Mark a task as completed.
async fn mark_task_completed(db: &Arc<BrainDb>, task_id: &str, result: &str) {
    let now = chrono::Utc::now().to_rfc3339();
    let task_id = task_id.to_string();
    let result = result.to_string();
    let _ = db.with_conn(move |conn| {
        conn.execute(
            "UPDATE swarm_tasks SET status = 'completed', result = ?1, completed_at = ?2 WHERE id = ?3",
            params![result, now, task_id],
        ).map_err(|e| BrainError::Database(e.to_string()))
    }).await;
}

/// Mark a task as failed.
async fn mark_task_failed(db: &Arc<BrainDb>, task_id: &str, reason: &str) {
    let now = chrono::Utc::now().to_rfc3339();
    let task_id = task_id.to_string();
    let reason = reason.to_string();
    let _ = db.with_conn(move |conn| {
        conn.execute(
            "UPDATE swarm_tasks SET status = 'failed', result = ?1, completed_at = ?2 WHERE id = ?3",
            params![reason, now, task_id],
        ).map_err(|e| BrainError::Database(e.to_string()))
    }).await;
}

/// Check if any goals have all tasks completed and send a summary.
async fn check_completed_goals(db: &Arc<BrainDb>) {
    // Find goals where all tasks are done
    let completed_goals: Vec<String> = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT parent_goal FROM swarm_tasks
             WHERE parent_goal IS NOT NULL
             GROUP BY parent_goal
             HAVING COUNT(*) = SUM(CASE WHEN status IN ('completed', 'failed') THEN 1 ELSE 0 END)"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(g) = r { result.push(g); } }
        Ok(result)
    }).await.unwrap_or_default();

    for goal_id in completed_goals {
        let _ = send_message(
            db,
            "orchestrator",
            "all",
            "complete",
            &format!("Goal fully completed: {}", goal_id),
            9,
        ).await;
        log::info!("Swarm: goal '{}' fully completed", goal_id);
    }
}

// =========================================================================
// Circuit integration
// =========================================================================

/// Circuit entry point — called by the circuits rotation engine.
pub async fn circuit_swarm_orchestrator(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    // Init tables if needed (idempotent)
    init_swarm(db).await?;

    // Run one tick
    orchestrator_tick(db).await?;

    // Return status summary
    let status = get_swarm_status_inner(db).await?;
    Ok(format!(
        "Swarm: {} agents, {} tasks ({} pending, {} in-progress, {} completed, {} failed)",
        status.agent_count,
        status.total_tasks,
        status.pending_tasks,
        status.in_progress_tasks,
        status.completed_tasks,
        status.failed_tasks,
    ))
}
