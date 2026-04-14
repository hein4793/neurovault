use crate::db::models::GraphNode;
use crate::db::BrainDb;
use crate::error::BrainError;
use crate::quality;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityReport {
    pub total_nodes: u64,
    pub avg_quality: f64,
    pub avg_decay: f64,
    pub low_quality_count: u64,
    pub high_quality_count: u64,
    pub decayed_count: u64,
}

#[tauri::command]
pub async fn calculate_quality(db: State<'_, Arc<BrainDb>>) -> Result<(u64, u64), BrainError> {
    quality::scoring::calculate_quality_scores(&db).await
}

#[tauri::command]
pub async fn calculate_decay(db: State<'_, Arc<BrainDb>>) -> Result<(u64, u64), BrainError> {
    quality::decay::calculate_decay_scores(&db).await
}

#[tauri::command]
pub async fn get_quality_report(db: State<'_, Arc<BrainDb>>) -> Result<QualityReport, BrainError> {
    let report = db.with_conn(|conn| {
        let (total, avg_quality, avg_decay): (u64, f64, f64) = conn.query_row(
            "SELECT COUNT(*), COALESCE(AVG(quality_score), 0.0), COALESCE(AVG(decay_score), 0.0) FROM nodes",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ).map_err(|e| BrainError::Database(e.to_string()))?;

        if total == 0 {
            return Ok(QualityReport {
                total_nodes: 0, avg_quality: 0.0, avg_decay: 0.0,
                low_quality_count: 0, high_quality_count: 0, decayed_count: 0,
            });
        }

        let low_q: u64 = conn.query_row(
            "SELECT COUNT(*) FROM nodes WHERE quality_score < 0.3",
            [],
            |row| row.get(0),
        ).map_err(|e| BrainError::Database(e.to_string()))?;

        let high_q: u64 = conn.query_row(
            "SELECT COUNT(*) FROM nodes WHERE quality_score > 0.7",
            [],
            |row| row.get(0),
        ).map_err(|e| BrainError::Database(e.to_string()))?;

        let decayed: u64 = conn.query_row(
            "SELECT COUNT(*) FROM nodes WHERE decay_score < 0.3",
            [],
            |row| row.get(0),
        ).map_err(|e| BrainError::Database(e.to_string()))?;

        Ok(QualityReport {
            total_nodes: total,
            avg_quality,
            avg_decay,
            low_quality_count: low_q,
            high_quality_count: high_q,
            decayed_count: decayed,
        })
    }).await?;

    Ok(report)
}

/// AI-powered quality enhancement: backfill summaries + tags, then recalculate scores.
/// Returns (summaries_done, tags_done, failed).
#[tauri::command]
pub async fn enhance_quality(
    db: State<'_, Arc<BrainDb>>,
) -> Result<(u64, u64, u64), BrainError> {
    let llm = crate::commands::ai::get_llm_client(&db);

    // Step 1: AI-backfill summaries for truncated nodes
    let (summaries_done, summaries_failed) =
        crate::ai::summarize::backfill_summaries(&db, &llm).await?;
    log::info!("Enhance: {} summaries generated, {} failed", summaries_done, summaries_failed);

    // Step 2: AI-extract tags for under-tagged nodes (only fetch what we need, capped at 30)
    #[derive(Debug)]
    struct TagCandidate { id: String, title: String, content: String, tags: Vec<String> }

    let candidates: Vec<TagCandidate> = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, title, content, tags FROM nodes \
             WHERE LENGTH(content) > 50 \
             AND (tags = '[]' OR tags = '' OR tags IS NULL \
                  OR LENGTH(tags) - LENGTH(REPLACE(tags, ',', '')) < 2) \
             LIMIT 30"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            let tags_json: String = row.get::<_, String>(3).unwrap_or_default();
            Ok(TagCandidate {
                id: row.get(0)?,
                title: row.get(1)?,
                content: row.get(2)?,
                tags: serde_json::from_str(&tags_json).unwrap_or_default(),
            })
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(n) = r { result.push(n); } }
        Ok(result)
    }).await?;

    let mut tags_done = 0u64;
    for node in &candidates {
        match llm.extract_tags(&node.content).await {
            Ok(new_tags) => {
                let mut merged = node.tags.clone();
                for tag in &new_tags {
                    if !merged.contains(tag) { merged.push(tag.clone()); }
                }
                let tags_json = serde_json::to_string(&merged).unwrap_or_else(|_| "[]".to_string());
                let id_clone = node.id.clone();
                let _ = db.with_conn(move |conn| {
                    conn.execute(
                        "UPDATE nodes SET tags = ?1 WHERE id = ?2",
                        params![tags_json, id_clone],
                    ).map_err(|e| BrainError::Database(e.to_string()))
                }).await;
                tags_done += 1;
            }
            Err(e) => {
                log::warn!("Failed to extract tags for {}: {}", node.title, e);
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    }

    // Step 3: Recalculate quality + decay scores
    quality::scoring::calculate_quality_scores(&db).await?;
    quality::decay::calculate_decay_scores(&db).await?;

    Ok((summaries_done, tags_done, summaries_failed))
}

/// Boost IQ by creating cross-domain edges and recalculating all scores.
/// Returns (cross_links_created, quality_updated, decay_updated).
#[tauri::command]
pub async fn boost_iq(
    db: State<'_, Arc<BrainDb>>,
) -> Result<(u64, u64, u64), BrainError> {
    // Step 1: Create cross-domain edges based on embedding similarity
    #[derive(Debug)]
    struct EmbNode {
        id: String,
        domain: String,
        embedding: Vec<f64>,
    }

    let emb_nodes: Vec<EmbNode> = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT n.id, n.domain, e.vector, e.dimension FROM nodes n \
             INNER JOIN embeddings e ON e.node_id = n.id"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let domain: String = row.get(1)?;
            let blob: Vec<u8> = row.get(2)?;
            let dim: usize = row.get(3)?;
            let embedding: Vec<f64> = blob.chunks_exact(8)
                .take(dim)
                .map(|chunk| f64::from_le_bytes(chunk.try_into().unwrap_or([0u8; 8])))
                .collect();
            Ok(EmbNode { id, domain, embedding })
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(n) = r { result.push(n); } }
        Ok(result)
    }).await?;

    // Get existing edges to avoid duplicates
    let existing: std::collections::HashSet<(String, String)> = db.with_conn(|conn| {
        let mut stmt = conn.prepare("SELECT source_id, target_id FROM edges")
            .map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut set = std::collections::HashSet::new();
        for r in rows {
            if let Ok((s, t)) = r {
                set.insert((s.clone(), t.clone()));
                set.insert((t, s));
            }
        }
        Ok(set)
    }).await?;

    let mut cross_links = 0u64;
    let now = chrono::Utc::now().to_rfc3339();

    // Compare nodes across different domains (sample for performance)
    let mut domains: std::collections::HashMap<String, Vec<usize>> = std::collections::HashMap::new();
    for (i, node) in emb_nodes.iter().enumerate() {
        domains.entry(node.domain.clone()).or_default().push(i);
    }

    let domain_keys: Vec<String> = domains.keys().cloned().collect();
    let mut edges_to_create: Vec<(String, String, f64, String)> = Vec::new();

    for i in 0..domain_keys.len() {
        for j in (i + 1)..domain_keys.len() {
            let nodes_a = &domains[&domain_keys[i]];
            let nodes_b = &domains[&domain_keys[j]];
            let limit_a = nodes_a.len().min(20);
            let limit_b = nodes_b.len().min(20);
            for &ai in nodes_a.iter().take(limit_a) {
                for &bi in nodes_b.iter().take(limit_b) {
                    let a = &emb_nodes[ai];
                    let b = &emb_nodes[bi];
                    if existing.contains(&(a.id.clone(), b.id.clone())) { continue; }

                    let sim = cosine_sim(&a.embedding, &b.embedding);
                    if sim > 0.6 {
                        let strength = ((sim - 0.4) * 1.5).min(0.9);
                        let evidence = format!("Cross-domain similarity: {:.0}%", sim * 100.0);
                        edges_to_create.push((a.id.clone(), b.id.clone(), strength, evidence));
                        cross_links += 1;
                        if cross_links >= 200 { break; }
                    }
                }
                if cross_links >= 200 { break; }
            }
            if cross_links >= 200 { break; }
        }
        if cross_links >= 200 { break; }
    }

    // Batch insert edges
    if !edges_to_create.is_empty() {
        let now_clone = now.clone();
        db.with_conn(move |conn| {
            for (src, tgt, strength, evidence) in &edges_to_create {
                let id = format!("edges:{}", uuid::Uuid::now_v7());
                conn.execute(
                    "INSERT OR IGNORE INTO edges (id, source_id, target_id, relation_type, strength, \
                     discovered_by, evidence, animated, created_at, traversal_count) \
                     VALUES (?1, ?2, ?3, 'cross_domain', ?4, 'iq_boost', ?5, 1, ?6, 0)",
                    params![id, src, tgt, strength, evidence, now_clone],
                ).map_err(|e| BrainError::Database(e.to_string()))?;
            }
            Ok(())
        }).await?;
    }

    // Step 2: Recalculate quality + decay
    let (q_updated, _) = quality::scoring::calculate_quality_scores(&db).await?;
    let (d_updated, _) = quality::decay::calculate_decay_scores(&db).await?;

    log::info!("Boost IQ: {} cross-domain links, {} quality, {} decay updated", cross_links, q_updated, d_updated);
    Ok((cross_links, q_updated, d_updated))
}

fn cosine_sim(a: &[f64], b: &[f64]) -> f64 {
    if a.len() != b.len() || a.is_empty() { return 0.0; }
    let dot: f64 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let ma: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let mb: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();
    if ma == 0.0 || mb == 0.0 { return 0.0; }
    (dot / (ma * mb)).clamp(0.0, 1.0)
}

#[tauri::command]
pub async fn merge_duplicate_nodes(
    db: State<'_, Arc<BrainDb>>,
    keep_id: String,
    remove_id: String,
) -> Result<GraphNode, BrainError> {
    quality::dedup::merge_nodes(&db, &keep_id, &remove_id).await
}

/// Deep Learn: auto-research real-world topics to build knowledge.
/// Returns (topics_researched, nodes_created).
#[tauri::command]
pub async fn deep_learn(
    db: State<'_, Arc<BrainDb>>,
) -> Result<(u64, u64), BrainError> {
    #[derive(Debug)]
    struct DomainRow { domain: String }

    let existing_topics: Vec<DomainRow> = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT topic AS domain FROM nodes WHERE source_type = 'research'"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok(DomainRow { domain: row.get(0)? })
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(d) = r { result.push(d); } }
        Ok(result)
    }).await.unwrap_or_default();

    let researched: std::collections::HashSet<String> = existing_topics.iter()
        .map(|r| r.domain.to_lowercase())
        .collect();

    // 1. Pull from the curiosity queue
    let curiosity = crate::learning::curiosity::generate_curiosity_queue(&db)
        .await
        .unwrap_or_default();

    // 2. Pull pending research_missions
    let missions: Vec<String> = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT topic FROM research_missions WHERE status = 'pending' ORDER BY created_at DESC LIMIT 20"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| row.get(0))
            .map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(t) = r { result.push(t); } }
        Ok(result)
    }).await.unwrap_or_default();

    // 3. Fall back to seed topics if empty
    let seed_topics = ["AI agents", "Tauri 2", "SQLite", "Ollama"];

    let mut topics: Vec<String> = Vec::new();
    for c in curiosity.iter().take(15) {
        if !researched.contains(&c.topic.to_lowercase()) && topics.len() < 20 {
            topics.push(c.topic.clone());
        }
    }
    for m in missions.iter().take(10) {
        if !researched.contains(&m.to_lowercase()) && topics.len() < 20 {
            topics.push(m.clone());
        }
    }
    if topics.is_empty() {
        for s in &seed_topics {
            topics.push(s.to_string());
        }
    }

    log::info!("Deep Learn: researching {} topics from curiosity + missions: {:?}", topics.len(), topics);

    let mut total_nodes = 0u64;
    let mut topics_done = 0u64;
    let db_arc: Arc<BrainDb> = Arc::clone(&db);

    for topic in &topics {
        match crate::commands::ingest::research_topic_inner(&db_arc, topic).await {
            Ok(nodes) => {
                total_nodes += nodes.len() as u64;
                topics_done += 1;
                log::info!("Deep Learn: '{}' → {} nodes", topic, nodes.len());
            }
            Err(e) => {
                log::warn!("Deep Learn: '{}' failed: {}", topic, e);
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }

    let _ = db.auto_link_nodes().await;
    let _ = quality::scoring::calculate_quality_scores(&db).await;
    let _ = quality::decay::calculate_decay_scores(&db).await;

    log::info!("Deep Learn complete: {} topics, {} nodes", topics_done, total_nodes);
    Ok((topics_done, total_nodes))
}

/// Quality Sweep: push nodes in the 0.5-0.7 band above 0.7 with AI summaries + tags.
/// Returns (nodes_improved, nodes_promoted_to_hq).
#[tauri::command]
pub async fn quality_sweep(
    db: State<'_, Arc<BrainDb>>,
) -> Result<(u64, u64), BrainError> {
    let llm = crate::commands::ai::get_llm_client(&db);

    // Find nodes in the "almost high quality" band
    let nodes: Vec<(String, String, String, Vec<String>)> = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, content, summary, tags FROM nodes \
             WHERE quality_score >= 0.45 AND quality_score < 0.70 \
             ORDER BY quality_score DESC LIMIT 100"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let content: String = row.get(1)?;
            let summary: String = row.get(2)?;
            let tags_json: String = row.get(3)?;
            let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
            Ok((id, content, summary, tags))
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(n) = r { result.push(n); } }
        Ok(result)
    }).await?;

    log::info!("Quality Sweep: {} nodes in 0.45-0.70 band", nodes.len());
    let mut improved = 0u64;

    for (id, content, summary, tags) in &nodes {
        let mut changed = false;

        // AI summarize if truncated
        if summary.ends_with("...") && content.len() > 50 {
            if let Ok(new_summary) = llm.summarize(content).await {
                let now = chrono::Utc::now().to_rfc3339();
                let id_clone = id.clone();
                let _ = db.with_conn(move |conn| {
                    conn.execute(
                        "UPDATE nodes SET summary = ?1, updated_at = ?2 WHERE id = ?3",
                        params![new_summary, now, id_clone],
                    ).map_err(|e| BrainError::Database(e.to_string()))
                }).await;
                changed = true;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;
        }

        // AI extract more tags if < 5
        if tags.len() < 5 && content.len() > 50 {
            if let Ok(new_tags) = llm.extract_tags(content).await {
                let mut merged = tags.clone();
                for tag in &new_tags {
                    if !merged.contains(tag) { merged.push(tag.clone()); }
                }
                let tags_json = serde_json::to_string(&merged).unwrap_or_else(|_| "[]".to_string());
                let id_clone = id.clone();
                let _ = db.with_conn(move |conn| {
                    conn.execute(
                        "UPDATE nodes SET tags = ?1 WHERE id = ?2",
                        params![tags_json, id_clone],
                    ).map_err(|e| BrainError::Database(e.to_string()))
                }).await;
                changed = true;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;
        }

        if changed { improved += 1; }
    }

    // Recalculate scores
    quality::scoring::calculate_quality_scores(&db).await?;
    quality::decay::calculate_decay_scores(&db).await?;

    // Count how many are now high quality
    let promoted: u64 = db.with_conn(|conn| {
        conn.query_row(
            "SELECT COUNT(*) FROM nodes WHERE quality_score > 0.7",
            [],
            |row| row.get(0),
        ).map_err(|e| BrainError::Database(e.to_string()))
    }).await?;

    log::info!("Quality Sweep: {} improved, {} total HQ nodes", improved, promoted);
    Ok((improved, promoted))
}

/// Maximize IQ: full optimization pipeline in one click.
#[tauri::command]
pub async fn maximize_iq(
    db: State<'_, Arc<BrainDb>>,
) -> Result<String, BrainError> {
    let mut report = String::new();

    report.push_str("Step 1: Deep Learning...\n");
    let (topics, nodes) = deep_learn(db.clone()).await.unwrap_or((0, 0));
    report.push_str(&format!("  Researched {} topics → {} new nodes\n", topics, nodes));

    report.push_str("Step 2: Quality Sweep...\n");
    let (improved, hq) = quality_sweep(db.clone()).await.unwrap_or((0, 0));
    report.push_str(&format!("  Improved {} nodes → {} total high-quality\n", improved, hq));

    report.push_str("Step 3: Cross-Domain Boost...\n");
    let (links, _, _) = boost_iq(db.clone()).await.unwrap_or((0, 0, 0));
    report.push_str(&format!("  Created {} cross-domain links\n", links));

    quality::scoring::calculate_quality_scores(&db).await?;
    quality::decay::calculate_decay_scores(&db).await?;

    report.push_str("Done! Refresh Insights to see new IQ.");
    log::info!("Maximize IQ complete:\n{}", report);
    Ok(report)
}
