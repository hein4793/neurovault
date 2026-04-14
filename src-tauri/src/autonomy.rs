//! Brain Autonomy Engine — background scheduler that makes the brain self-improving.
//!
//! Runs a 60-second tick loop. Each tick checks if enough time has elapsed
//! for each task category, runs it, and persists state to SQLite.

use crate::commands::settings::load_settings;
use crate::db::BrainDb;
use crate::events::{emit_event, BrainEvent};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

/// Persisted state for a single autonomy task.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TaskState {
    task_name: String,
    last_run_at: String,
    last_result: String,
    runs_today: u32,
    today_date: String,
}

/// In-memory tracker for the loop.
struct TaskTracker {
    last_run: HashMap<String, Instant>,
}

impl TaskTracker {
    fn new() -> Self {
        Self {
            last_run: HashMap::new(),
        }
    }

    /// Check if a task is due based on interval in minutes.
    fn is_due(&self, task: &str, interval_mins: u64) -> bool {
        match self.last_run.get(task) {
            Some(last) => last.elapsed().as_secs() >= interval_mins * 60,
            None => true, // Never run → due immediately on first tick
        }
    }

    fn mark_run(&mut self, task: &str) {
        self.last_run.insert(task.to_string(), Instant::now());
    }
}

/// Load persisted task states from SQLite to resume after restart.
async fn load_persisted_states(db: &BrainDb) -> HashMap<String, TaskState> {
    let states: Vec<TaskState> = db
        .with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT task_name, last_run_at, last_result, runs_today, today_date FROM autonomy_state",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(TaskState {
                    task_name: row.get(0)?,
                    last_run_at: row.get(1)?,
                    last_result: row.get(2)?,
                    runs_today: row.get(3)?,
                    today_date: row.get(4)?,
                })
            })?;
            let mut result = Vec::new();
            for row in rows {
                if let Ok(state) = row {
                    result.push(state);
                }
            }
            Ok(result)
        })
        .await
        .unwrap_or_default();

    states.into_iter().map(|s| (s.task_name.clone(), s)).collect()
}

/// Persist task state after a run.
async fn save_task_state(db: &BrainDb, task: &str, result: &str) {
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let task_owned = task.to_string();
    let result_owned = result.to_string();

    // Upsert: increment runs_today if same day, else reset to 1
    let _ = db
        .with_conn(move |conn| {
            // Check if a row already exists
            let exists: bool = conn
                .prepare("SELECT 1 FROM autonomy_state WHERE task_name = ?1")?
                .exists(params![task_owned])?;

            if exists {
                conn.execute(
                    "UPDATE autonomy_state SET
                        last_run_at = ?1,
                        last_result = ?2,
                        runs_today = CASE WHEN today_date = ?3 THEN runs_today + 1 ELSE 1 END,
                        today_date = ?3
                     WHERE task_name = ?4",
                    params![now, result_owned, today, task_owned],
                )?;
            } else {
                conn.execute(
                    "INSERT INTO autonomy_state (task_name, last_run_at, last_result, runs_today, today_date)
                     VALUES (?1, ?2, ?3, 1, ?4)",
                    params![task_owned, now, result_owned, today],
                )?;
            }
            Ok(())
        })
        .await;
}

/// Get today's total research count from persisted state.
async fn get_daily_research_count(db: &BrainDb) -> u32 {
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    db.with_conn(move |conn| {
        let result: Result<u32, _> = conn.query_row(
            "SELECT runs_today FROM autonomy_state WHERE task_name = 'active_learning' AND today_date = ?1",
            params![today],
            |row| row.get(0),
        );
        Ok(result.unwrap_or(0))
    })
    .await
    .unwrap_or(0)
}

/// Main autonomy loop — spawned as a background task at app startup.
pub async fn run_autonomy_loop(db: Arc<BrainDb>, app: tauri::AppHandle) {
    log::info!("Autonomy engine starting...");

    // Wait for DB to settle after startup
    tokio::time::sleep(std::time::Duration::from_secs(30)).await;

    let mut tracker = TaskTracker::new();

    // Load persisted states and pre-fill tracker to respect last-run times
    let persisted = load_persisted_states(&db).await;
    for (name, state) in &persisted {
        // If the task ran recently (within its interval), skip the first run
        if let Ok(last) = chrono::DateTime::parse_from_rfc3339(&state.last_run_at) {
            let elapsed_secs = (chrono::Utc::now() - last.with_timezone(&chrono::Utc))
                .num_seconds()
                .max(0) as u64;
            // Fake the last_run Instant so the tracker knows about prior runs
            if elapsed_secs < 86400 {
                // Create an Instant that represents when the task last ran
                let now = Instant::now();
                let fake_instant = now.checked_sub(std::time::Duration::from_secs(elapsed_secs));
                if let Some(inst) = fake_instant {
                    tracker.last_run.insert(name.clone(), inst);
                }
            }
        }
    }

    log::info!("Autonomy engine ready. Loaded {} persisted task states.", persisted.len());

    // Run initial export shortly after startup (5 min delay to let embedding pipeline settle)
    tokio::time::sleep(std::time::Duration::from_secs(270)).await;

    loop {
        // Read settings each tick so user can toggle on/off without restart
        let settings = load_settings(&db);

        if !settings.autonomy_enabled {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            continue;
        }

        // === Auto-link ===
        if tracker.is_due("auto_link", settings.autonomy_linking_mins) {
            run_task(&db, &app, &mut tracker, "auto_link", || {
                let db = db.clone();
                async move {
                    let result = db.auto_link_nodes().await?;
                    Ok(format!("Created {} synapses", result.created))
                }
            })
            .await;
        }

        // === Quality + Decay recalculation ===
        if tracker.is_due("quality_recalc", settings.autonomy_quality_mins / 2) {
            run_task(&db, &app, &mut tracker, "quality_recalc", || {
                let db = db.clone();
                async move {
                    let (q_updated, _) = crate::quality::scoring::calculate_quality_scores(&db).await?;
                    let (d_updated, _) = crate::quality::decay::calculate_decay_scores(&db).await?;
                    Ok(format!("{} quality + {} decay scores updated", q_updated, d_updated))
                }
            })
            .await;
        }

        // === Quality sweep (AI-powered, less frequent) ===
        if tracker.is_due("quality_sweep", settings.autonomy_quality_mins) {
            run_task(&db, &app, &mut tracker, "quality_sweep", || {
                let db = db.clone();
                async move {
                    let llm = crate::commands::ai::get_llm_client(&db);
                    // Find improvable nodes (0.45-0.70 band)
                    let nodes: Vec<crate::db::models::KnowledgeNode> = db
                        .with_conn(|conn| {
                            let mut stmt = conn.prepare(
                                "SELECT id, title, content, summary, content_hash, domain, topic, tags,
                                        node_type, source_type, source_url, source_file, quality_score,
                                        visual_size, cluster_id, created_at, updated_at, accessed_at,
                                        access_count, decay_score, embedding, synthesized_by_brain,
                                        cognitive_type, confidence, memory_tier, compression_parent, brain_id
                                 FROM nodes
                                 WHERE quality_score >= 0.45 AND quality_score < 0.70
                                 ORDER BY quality_score DESC
                                 LIMIT 20",
                            )?;
                            let rows = stmt.query_map([], |row| {
                                let tags_str: String = row.get(7)?;
                                let tags: Vec<String> =
                                    serde_json::from_str(&tags_str).unwrap_or_default();
                                let embedding_str: Option<String> = row.get(20)?;
                                let embedding: Option<Vec<f64>> = embedding_str
                                    .as_deref()
                                    .and_then(|s| serde_json::from_str(s).ok());
                                let synth_int: i32 = row.get(21)?;
                                Ok(crate::db::models::KnowledgeNode {
                                    id: row.get(0)?,
                                    title: row.get(1)?,
                                    content: row.get(2)?,
                                    summary: row.get(3)?,
                                    content_hash: row.get(4)?,
                                    domain: row.get(5)?,
                                    topic: row.get(6)?,
                                    tags,
                                    node_type: row.get(8)?,
                                    source_type: row.get(9)?,
                                    source_url: row.get(10)?,
                                    source_file: row.get(11)?,
                                    quality_score: row.get(12)?,
                                    visual_size: row.get(13)?,
                                    cluster_id: row.get(14)?,
                                    created_at: row.get(15)?,
                                    updated_at: row.get(16)?,
                                    accessed_at: row.get(17)?,
                                    access_count: row.get(18)?,
                                    decay_score: row.get(19)?,
                                    embedding,
                                    synthesized_by_brain: synth_int != 0,
                                    cognitive_type: row.get(22)?,
                                    confidence: row.get(23)?,
                                    memory_tier: row.get(24)?,
                                    compression_parent: row.get(25)?,
                                    brain_id: row.get(26)?,
                                })
                            })?;
                            let mut result = Vec::new();
                            for row in rows {
                                if let Ok(node) = row {
                                    result.push(node);
                                }
                            }
                            Ok(result)
                        })
                        .await?;

                    let mut improved = 0u64;
                    for node in &nodes {
                        let id = match &node.id {
                            Some(id) => id.clone(),
                            None => continue,
                        };

                        let mut changed = false;

                        // AI summarize if truncated
                        if node.summary.ends_with("...") && node.content.len() > 50 {
                            if let Ok(summary) = llm.summarize(&node.content).await {
                                let now_ts = chrono::Utc::now().to_rfc3339();
                                let id_c = id.clone();
                                let _ = db
                                    .with_conn(move |conn| {
                                        conn.execute(
                                            "UPDATE nodes SET summary = ?1, updated_at = ?2 WHERE id = ?3",
                                            params![summary, now_ts, id_c],
                                        )?;
                                        Ok(())
                                    })
                                    .await;
                                changed = true;
                            }
                            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                        }

                        // AI extract more tags if < 5
                        if node.tags.len() < 5 && node.content.len() > 50 {
                            if let Ok(new_tags) = llm.extract_tags(&node.content).await {
                                let mut merged = node.tags.clone();
                                for tag in &new_tags {
                                    if !merged.contains(tag) { merged.push(tag.clone()); }
                                }
                                let tags_json = serde_json::to_string(&merged).unwrap_or_else(|_| "[]".to_string());
                                let id_c = id.clone();
                                let _ = db
                                    .with_conn(move |conn| {
                                        conn.execute(
                                            "UPDATE nodes SET tags = ?1 WHERE id = ?2",
                                            params![tags_json, id_c],
                                        )?;
                                        Ok(())
                                    })
                                    .await;
                                changed = true;
                            }
                            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                        }

                        if changed { improved += 1; }
                    }

                    // Recalculate after improvements
                    if improved > 0 {
                        crate::quality::scoring::calculate_quality_scores(&db).await?;
                    }

                    Ok(format!("Improved {} nodes", improved))
                }
            })
            .await;
        }

        // === IQ Boost (cross-domain links) ===
        if tracker.is_due("iq_boost", settings.autonomy_learning_mins) {
            run_task(&db, &app, &mut tracker, "iq_boost", || {
                let db = db.clone();
                async move {
                    // Fetch nodes with embeddings for cross-domain linking
                    #[derive(Debug)]
                    struct EmbNode {
                        id: String,
                        domain: String,
                        embedding: Vec<f64>,
                    }

                    let emb_nodes: Vec<EmbNode> = db
                        .with_conn(|conn| {
                            let mut stmt = conn.prepare(
                                "SELECT id, domain, embedding FROM nodes WHERE embedding IS NOT NULL AND embedding != ''",
                            )?;
                            let rows = stmt.query_map([], |row| {
                                let emb_str: String = row.get(2)?;
                                let embedding: Vec<f64> =
                                    serde_json::from_str(&emb_str).unwrap_or_default();
                                Ok(EmbNode {
                                    id: row.get(0)?,
                                    domain: row.get(1)?,
                                    embedding,
                                })
                            })?;
                            let mut result = Vec::new();
                            for row in rows {
                                if let Ok(node) = row {
                                    if !node.embedding.is_empty() {
                                        result.push(node);
                                    }
                                }
                            }
                            Ok(result)
                        })
                        .await?;

                    let edges: Vec<crate::db::models::KnowledgeEdge> = db
                        .with_conn(|conn| {
                            let mut stmt = conn.prepare(
                                "SELECT id, source_id, target_id, relation_type, strength, discovered_by, evidence, animated, created_at, traversal_count FROM edges",
                            )?;
                            let rows = stmt.query_map([], |row| {
                                let animated_int: i32 = row.get(7)?;
                                Ok(crate::db::models::KnowledgeEdge {
                                    id: row.get(0)?,
                                    source_id: row.get(1)?,
                                    target_id: row.get(2)?,
                                    relation_type: row.get(3)?,
                                    strength: row.get(4)?,
                                    discovered_by: row.get(5)?,
                                    evidence: row.get(6)?,
                                    animated: animated_int != 0,
                                    created_at: row.get(8)?,
                                    traversal_count: row.get(9)?,
                                })
                            })?;
                            let mut result = Vec::new();
                            for row in rows {
                                if let Ok(edge) = row {
                                    result.push(edge);
                                }
                            }
                            Ok(result)
                        })
                        .await?;

                    let mut existing: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
                    for e in &edges {
                        existing.insert((e.source_id.clone(), e.target_id.clone()));
                        existing.insert((e.target_id.clone(), e.source_id.clone()));
                    }

                    let mut cross_links = 0u64;
                    let now = chrono::Utc::now().to_rfc3339();

                    let mut domains: std::collections::HashMap<String, Vec<usize>> = std::collections::HashMap::new();
                    for (i, node) in emb_nodes.iter().enumerate() {
                        domains.entry(node.domain.clone()).or_default().push(i);
                    }

                    let domain_keys: Vec<String> = domains.keys().cloned().collect();
                    'outer: for i in 0..domain_keys.len() {
                        for j in (i + 1)..domain_keys.len() {
                            let nodes_a = &domains[&domain_keys[i]];
                            let nodes_b = &domains[&domain_keys[j]];
                            for &ai in nodes_a.iter().take(15) {
                                for &bi in nodes_b.iter().take(15) {
                                    let a = &emb_nodes[ai];
                                    let b = &emb_nodes[bi];
                                    let id_a = a.id.clone();
                                    let id_b = b.id.clone();
                                    if existing.contains(&(id_a.clone(), id_b.clone())) { continue; }

                                    let sim = cosine_sim(&a.embedding, &b.embedding);
                                    if sim > 0.6 {
                                        let strength = ((sim - 0.4) * 1.5).min(0.9);
                                        let evidence = format!("Autonomy cross-domain: {:.0}% similarity", sim * 100.0);
                                        let edge_id = format!("edges:{}", uuid::Uuid::now_v7());
                                        let src = id_a.clone();
                                        let tgt = id_b.clone();
                                        let now_c = now.clone();
                                        let _ = db
                                            .with_conn(move |conn| {
                                                conn.execute(
                                                    "INSERT INTO edges (id, source_id, target_id, relation_type, strength, discovered_by, evidence, animated, created_at, traversal_count)
                                                     VALUES (?1, ?2, ?3, 'cross_domain', ?4, 'autonomy', ?5, 1, ?6, 0)",
                                                    params![edge_id, src, tgt, strength, evidence, now_c],
                                                )?;
                                                Ok(())
                                            })
                                            .await;

                                        existing.insert((id_a, id_b));
                                        cross_links += 1;
                                        if cross_links >= 50 { break 'outer; }
                                    }
                                }
                            }
                        }
                    }

                    Ok(format!("Created {} cross-domain links", cross_links))
                }
            })
            .await;
        }

        // === Research Missions (execute pending missions queued by master loop) ===
        if tracker.is_due("research_missions", settings.autonomy_learning_mins) {
            run_task(&db, &app, &mut tracker, "research_missions", || {
                let db = db.clone();
                async move {
                    // Find up to 3 pending missions
                    struct Mission { id: String, topic: String }
                    let missions: Vec<Mission> = db.with_conn(|conn| {
                        let mut stmt = conn.prepare(
                            "SELECT id, topic FROM research_missions WHERE status = 'pending' ORDER BY priority DESC LIMIT 3"
                        )?;
                        let rows = stmt.query_map([], |row| {
                            Ok(Mission { id: row.get(0)?, topic: row.get(1)? })
                        })?;
                        let mut result = Vec::new();
                        for r in rows { if let Ok(m) = r { result.push(m); } }
                        Ok(result)
                    }).await?;

                    if missions.is_empty() {
                        return Ok("No pending missions".to_string());
                    }

                    let mut completed = 0u32;
                    let mut failed = 0u32;

                    for mission in &missions {
                        match crate::commands::ingest::research_topic_inner(&db, &mission.topic).await {
                            Ok(nodes) => {
                                let now = chrono::Utc::now().to_rfc3339();
                                let result_text = format!("Created {} nodes", nodes.len());
                                let mid = mission.id.clone();
                                let _ = db.with_conn(move |conn| {
                                    conn.execute(
                                        "UPDATE research_missions SET status = 'completed', completed_at = ?1, result = ?2 WHERE id = ?3",
                                        params![now, result_text, mid],
                                    )?;
                                    Ok(())
                                }).await;
                                completed += 1;
                                log::info!("Research mission '{}' completed: {} nodes", mission.topic, nodes.len());
                            }
                            Err(e) => {
                                let err_msg = format!("Failed: {}", e);
                                let mid = mission.id.clone();
                                let _ = db.with_conn(move |conn| {
                                    conn.execute(
                                        "UPDATE research_missions SET status = 'failed', result = ?1 WHERE id = ?2",
                                        params![err_msg, mid],
                                    )?;
                                    Ok(())
                                }).await;
                                failed += 1;
                                log::warn!("Research mission '{}' failed: {}", mission.topic, e);
                            }
                        }
                        // Rate limit between missions
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    }

                    // Auto-link if we created nodes
                    if completed > 0 {
                        let _ = db.auto_link_nodes().await;
                    }

                    Ok(format!("Missions: {} completed, {} failed", completed, failed))
                }
            })
            .await;
        }

        // === Active Learning (research new topics) ===
        if tracker.is_due("active_learning", settings.autonomy_learning_mins) {
            let daily_count = get_daily_research_count(&db).await;
            let max_daily = settings.autonomy_max_daily_research;

            if daily_count < max_daily {
                run_task(&db, &app, &mut tracker, "active_learning", || {
                    let db = db.clone();
                    let app = app.clone();
                    let remaining = (max_daily - daily_count).min(5) as usize;
                    async move {
                        let curiosity = crate::learning::curiosity::generate_curiosity_queue(&db).await?;

                        // Filter out already-researched topics
                        let logged_topics: Vec<String> = db
                            .with_conn(|conn| {
                                let mut stmt = conn.prepare("SELECT topic FROM learning_log")?;
                                let rows = stmt.query_map([], |row| {
                                    let topic: String = row.get(0)?;
                                    Ok(topic)
                                })?;
                                let mut result = Vec::new();
                                for row in rows {
                                    if let Ok(topic) = row {
                                        result.push(topic);
                                    }
                                }
                                Ok(result)
                            })
                            .await?;
                        let researched: std::collections::HashSet<String> = logged_topics
                            .iter()
                            .map(|r| r.to_lowercase())
                            .collect();

                        // Collect owned topic strings to avoid lifetime issues
                        let topics: Vec<String> = curiosity.iter()
                            .filter(|c| !researched.contains(&c.topic.to_lowercase()))
                            .take(remaining)
                            .map(|c| c.topic.clone())
                            .collect();

                        let mut total_nodes = 0u32;
                        let mut topics_done = 0u32;

                        for topic in &topics {
                            emit_event(&app, BrainEvent::ResearchStarted { topic: topic.clone() });

                            match crate::commands::ingest::research_topic_inner(&db, topic).await {
                                Ok(nodes) => {
                                    let count = nodes.len() as u32;
                                    total_nodes += count;
                                    topics_done += 1;

                                    // Log to learning_log
                                    let log_id = format!("learning_log:{}", uuid::Uuid::now_v7());
                                    let learned_at = chrono::Utc::now().to_rfc3339();
                                    let topic_c = topic.clone();
                                    let content = format!("{} nodes created", count);
                                    let _ = db
                                        .with_conn(move |conn| {
                                            conn.execute(
                                                "INSERT INTO learning_log (id, topic, content, learned_at, source)
                                                 VALUES (?1, ?2, ?3, ?4, 'autonomy')",
                                                params![log_id, topic_c, content, learned_at],
                                            )?;
                                            Ok(())
                                        })
                                        .await;

                                    emit_event(&app, BrainEvent::ResearchCompleted { topic: topic.clone(), nodes_created: count });
                                    log::info!("Autonomy learned: '{}' → {} nodes", topic, count);
                                }
                                Err(e) => {
                                    log::warn!("Autonomy learning failed for '{}': {}", topic, e);
                                }
                            }

                            // Rate limit between topics
                            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                        }

                        // Auto-link new knowledge
                        if total_nodes > 0 {
                            let _ = db.auto_link_nodes().await;
                        }

                        emit_event(&app, BrainEvent::ActiveLearningCompleted {
                            topics_researched: topics_done,
                            nodes_created: total_nodes,
                        });

                        Ok(format!("Researched {} topics → {} nodes", topics_done, total_nodes))
                    }
                })
                .await;
            }
        }

        // === Export + Briefing ===
        if tracker.is_due("export", settings.autonomy_export_mins) {
            run_task(&db, &app, &mut tracker, "export", || {
                let db = db.clone();
                let app = app.clone();
                async move {
                    crate::export::run_full_export(&db).await?;

                    // Get IQ + node count for event
                    let trends = crate::analysis::trends::analyze_trends(&db).await.ok();
                    let total: u64 = db
                        .with_conn(|conn| {
                            let count: u64 = conn.query_row(
                                "SELECT COUNT(*) FROM nodes",
                                [],
                                |row| row.get(0),
                            )?;
                            Ok(count)
                        })
                        .await?;
                    let iq = trends.map(|t| t.brain_iq).unwrap_or(0.0);

                    emit_event(&app, BrainEvent::BriefingUpdated { iq, total_nodes: total });

                    Ok(format!("Full export completed ({} nodes, IQ {:.0})", total, iq))
                }
            })
            .await;
        }

        // === User Profile Synthesis (every 6 hours) ===
        if tracker.is_due("user_profile", 360) {
            run_task(&db, &app, &mut tracker, "user_profile", || {
                let db = db.clone();
                async move {
                    crate::user::profile::synthesize_profile(&db).await
                }
            })
            .await;
        }

        // === Knowledge Synthesis (every 12 hours) ===
        if tracker.is_due("synthesis", 720) {
            run_task(&db, &app, &mut tracker, "synthesis", || {
                let db = db.clone();
                async move {
                    crate::ai::synthesis::run_synthesis(&db).await
                }
            })
            .await;
        }

        // === Phase 2.3 — Personal training export (daily) ===
        if tracker.is_due("personal_training_export", 1440) {
            run_task(&db, &app, &mut tracker, "personal_training_export", || {
                let db = db.clone();
                async move {
                    let path = db.config.export_dir().join("training-personal.jsonl");
                    let count = crate::export::training::export_personal_training(
                        &db,
                        &path.to_string_lossy(),
                    )
                    .await?;
                    Ok(format!("Wrote {} personal Q&A pairs to {}", count, path.display()))
                }
            })
            .await;
        }

        // === DB Maintenance (weekly) ===
        if tracker.is_due("db_maintenance", 1440 * 7) {
            run_task(&db, &app, &mut tracker, "db_maintenance", || {
                let db = db.clone();
                async move {
                    let archived = crate::quality::archive::archive_low_quality_nodes(&db).await.unwrap_or(0);
                    let deduped = crate::quality::archive::auto_deduplicate(&db).await.unwrap_or(0);
                    let stripped = crate::quality::archive::strip_low_value_embeddings(&db).await.unwrap_or(0);
                    Ok(format!("Archived {}, deduped {}, stripped {} embeddings", archived, deduped, stripped))
                }
            })
            .await;
        }

        // === Rotating Circuit Dispatcher (Phase 0 — every 20 minutes) ===
        //
        // Picks one of seven self-improvement circuits, never repeating
        // the last 3. This is the master plan's "20-min rotating scheduler"
        // that takes the brain from ~12 to 72 improvement cycles per day.
        //
        // It runs *additively* alongside the existing fixed-interval tasks
        // above so nothing is lost. Each circuit is logged to
        // autonomy_circuit_log and the rotation state to
        // autonomy_circuit_rotation.
        if tracker.is_due("circuit_dispatch", 20) {
            run_task(&db, &app, &mut tracker, "circuit_dispatch", || {
                let db = db.clone();
                async move {
                    let outcome = crate::circuits::dispatch_next_circuit(&db).await;
                    Ok(format!(
                        "circuit={} status={} ({}ms): {}",
                        outcome.circuit_name, outcome.status, outcome.duration_ms, outcome.result
                    ))
                }
            })
            .await;
        }

        // Sleep until next tick
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
    }
}

/// Run a single autonomy task with event emission, timing, error handling, and state persistence.
async fn run_task<F, Fut>(
    db: &Arc<BrainDb>,
    app: &tauri::AppHandle,
    tracker: &mut TaskTracker,
    task_name: &str,
    task_fn: F,
) where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<String, crate::error::BrainError>>,
{
    log::info!("Autonomy: starting task '{}'", task_name);
    emit_event(app, BrainEvent::AutonomyTaskStarted {
        task: task_name.to_string(),
    });

    let start = Instant::now();

    match task_fn().await {
        Ok(result) => {
            let duration_ms = start.elapsed().as_millis() as u64;
            log::info!("Autonomy: '{}' completed in {}ms: {}", task_name, duration_ms, result);
            emit_event(app, BrainEvent::AutonomyTaskCompleted {
                task: task_name.to_string(),
                result: result.clone(),
                duration_ms,
            });
            save_task_state(db, task_name, &result).await;
            tracker.mark_run(task_name);
        }
        Err(e) => {
            let error_msg = format!("{}", e);
            log::warn!("Autonomy: '{}' failed: {}", task_name, error_msg);
            emit_event(app, BrainEvent::AutonomyTaskFailed {
                task: task_name.to_string(),
                error: error_msg.clone(),
            });
            save_task_state(db, task_name, &format!("FAILED: {}", error_msg)).await;
            // Still mark as run to prevent rapid retries
            tracker.mark_run(task_name);
        }
    }
}

fn cosine_sim(a: &[f64], b: &[f64]) -> f64 {
    if a.len() != b.len() || a.is_empty() { return 0.0; }
    let dot: f64 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let ma: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let mb: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();
    if ma == 0.0 || mb == 0.0 { return 0.0; }
    (dot / (ma * mb)).clamp(0.0, 1.0)
}
