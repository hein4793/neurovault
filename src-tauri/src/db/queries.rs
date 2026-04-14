use super::models::*;
use super::BrainDb;
use crate::error::BrainError;
use rusqlite::params;
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Helper: parse a JSON array string into Vec<String>.
fn parse_tags(s: &str) -> Vec<String> {
    serde_json::from_str(s).unwrap_or_default()
}

impl BrainDb {
    /// Create a new knowledge node — with dedup protection.
    pub async fn create_node(&self, input: CreateNodeInput) -> Result<GraphNode, BrainError> {
        let now = chrono::Utc::now().to_rfc3339();
        let content_hash = format!("{:x}", Sha256::digest(input.content.as_bytes()));

        let result = self.with_conn(move |conn| {
            // DEDUP CHECK
            let mut stmt = conn.prepare(
                "SELECT id, title, content, summary, domain, topic, tags, node_type, source_type,
                        visual_size, access_count, decay_score, created_at
                 FROM nodes WHERE content_hash = ?1 LIMIT 1"
            ).map_err(|e| BrainError::Database(e.to_string()))?;

            let existing = stmt.query_row(params![content_hash], |row| {
                Ok(GraphNode {
                    id: row.get::<_, String>(0)?,
                    title: row.get::<_, String>(1)?,
                    content: row.get::<_, String>(2)?,
                    summary: row.get::<_, String>(3)?,
                    domain: row.get::<_, String>(4)?,
                    topic: row.get::<_, String>(5)?,
                    tags: parse_tags(&row.get::<_, String>(6).unwrap_or_default()),
                    node_type: row.get::<_, String>(7)?,
                    source_type: row.get::<_, String>(8)?,
                    visual_size: row.get::<_, f64>(9)?,
                    access_count: row.get::<_, u64>(10)?,
                    decay_score: row.get::<_, f64>(11)?,
                    created_at: row.get::<_, String>(12)?,
                })
            }).ok();

            if let Some(mut existing_node) = existing {
                conn.execute(
                    "UPDATE nodes SET accessed_at = ?1, access_count = access_count + 1 WHERE id = ?2",
                    params![now, existing_node.id],
                ).ok();
                existing_node.access_count += 1;
                log::debug!("Dedup: content_hash {} already exists as {}", &content_hash[..12], existing_node.id);
                return Ok(existing_node);
            }

            // CREATE NEW NODE
            let id = format!("node:{}", Uuid::now_v7());
            let summary = if input.content.len() > 200 {
                format!("{}...", crate::truncate_str(&input.content, 200))
            } else {
                input.content.clone()
            };
            let tags_json = serde_json::to_string(&input.tags).unwrap_or_else(|_| "[]".to_string());

            conn.execute(
                "INSERT INTO nodes (id, title, content, summary, content_hash, domain, topic, tags,
                                    node_type, source_type, source_url, quality_score, visual_size,
                                    decay_score, access_count, synthesized_by_brain,
                                    created_at, updated_at, accessed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 0.7, 3.0, 1.0, 0, 0, ?12, ?12, ?12)",
                params![
                    id, input.title, input.content, summary, content_hash,
                    input.domain, input.topic, tags_json, input.node_type,
                    input.source_type, input.source_url, now
                ],
            ).map_err(|e| BrainError::Database(e.to_string()))?;

            Ok(GraphNode {
                id,
                title: input.title,
                content: input.content,
                summary,
                domain: input.domain,
                topic: input.topic,
                tags: input.tags,
                node_type: input.node_type,
                source_type: input.source_type,
                visual_size: 3.0,
                access_count: 0,
                decay_score: 1.0,
                created_at: now,
            })
        }).await?;

        // Write to Obsidian-style vault (fire-and-forget — DB is authoritative)
        let config = self.config.clone();
        let vault_node = result.clone();
        std::thread::spawn(move || {
            let tags = vault_node.tags.clone();
            crate::vault::write_node_to_vault(&config, &vault_node, &tags);
        });

        Ok(result)
    }

    pub async fn get_edge_count(&self) -> Result<u64, BrainError> {
        self.with_conn(|conn| {
            let count: u64 = conn.query_row("SELECT COUNT(*) FROM edges", [], |row| row.get(0))
                .map_err(|e| BrainError::Database(e.to_string()))?;
            Ok(count)
        }).await
    }

    pub async fn get_node_count(&self) -> Result<u64, BrainError> {
        self.with_conn(|conn| {
            let count: u64 = conn.query_row("SELECT COUNT(*) FROM nodes", [], |row| row.get(0))
                .map_err(|e| BrainError::Database(e.to_string()))?;
            Ok(count)
        }).await
    }

    pub async fn get_nodes_paginated(&self, offset: u64, limit: u64) -> Result<Vec<GraphNode>, BrainError> {
        self.with_conn(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, title, summary, domain, topic, tags, node_type, source_type,
                        visual_size, access_count, decay_score, created_at
                 FROM nodes ORDER BY quality_score DESC LIMIT ?1 OFFSET ?2"
            ).map_err(|e| BrainError::Database(e.to_string()))?;

            let rows = stmt.query_map(params![limit, offset], |row| {
                Ok(GraphNode {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    content: String::new(),
                    summary: row.get(2)?,
                    domain: row.get(3)?,
                    topic: row.get(4)?,
                    tags: parse_tags(&row.get::<_, String>(5).unwrap_or_default()),
                    node_type: row.get(6)?,
                    source_type: row.get(7)?,
                    visual_size: row.get(8)?,
                    access_count: row.get(9)?,
                    decay_score: row.get(10)?,
                    created_at: row.get(11)?,
                })
            }).map_err(|e| BrainError::Database(e.to_string()))?;

            let mut results = Vec::new();
            for row in rows {
                results.push(row.map_err(|e| BrainError::Database(e.to_string()))?);
            }
            Ok(results)
        }).await
    }

    pub async fn get_all_nodes(&self) -> Result<Vec<GraphNode>, BrainError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, title, summary, domain, topic, tags, node_type, source_type,
                        visual_size, access_count, decay_score, created_at
                 FROM nodes ORDER BY quality_score DESC LIMIT 600"
            ).map_err(|e| BrainError::Database(e.to_string()))?;

            let rows = stmt.query_map([], |row| {
                Ok(GraphNode {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    content: String::new(),
                    summary: row.get(2)?,
                    domain: row.get(3)?,
                    topic: row.get(4)?,
                    tags: parse_tags(&row.get::<_, String>(5).unwrap_or_default()),
                    node_type: row.get(6)?,
                    source_type: row.get(7)?,
                    visual_size: row.get(8)?,
                    access_count: row.get(9)?,
                    decay_score: row.get(10)?,
                    created_at: row.get(11)?,
                })
            }).map_err(|e| BrainError::Database(e.to_string()))?;

            let mut results = Vec::new();
            for row in rows {
                results.push(row.map_err(|e| BrainError::Database(e.to_string()))?);
            }
            Ok(results)
        }).await
    }

    pub async fn get_all_edges(&self) -> Result<Vec<GraphEdge>, BrainError> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, source_id, target_id, relation_type, strength, animated
                 FROM edges ORDER BY strength DESC LIMIT 800"
            ).map_err(|e| BrainError::Database(e.to_string()))?;

            let rows = stmt.query_map([], |row| {
                Ok(GraphEdge {
                    id: row.get(0)?,
                    source: row.get(1)?,
                    target: row.get(2)?,
                    relation_type: row.get(3)?,
                    strength: row.get(4)?,
                    animated: row.get::<_, i32>(5)? != 0,
                })
            }).map_err(|e| BrainError::Database(e.to_string()))?;

            let mut results = Vec::new();
            for row in rows {
                results.push(row.map_err(|e| BrainError::Database(e.to_string()))?);
            }
            Ok(results)
        }).await
    }

    pub async fn create_edge(&self, input: CreateEdgeInput) -> Result<GraphEdge, BrainError> {
        let now = chrono::Utc::now().to_rfc3339();
        let id = format!("edge:{}", Uuid::now_v7());

        self.with_conn(move |conn| {
            conn.execute(
                "INSERT INTO edges (id, source_id, target_id, relation_type, strength,
                                    discovered_by, evidence, animated, created_at, traversal_count)
                 VALUES (?1, ?2, ?3, ?4, 0.5, 'user_created', ?5, 1, ?6, 0)",
                params![id, input.source_id, input.target_id, input.relation_type, input.evidence, now],
            ).map_err(|e| BrainError::Database(e.to_string()))?;

            Ok(GraphEdge {
                id,
                source: input.source_id,
                target: input.target_id,
                relation_type: input.relation_type,
                strength: 0.5,
                animated: true,
            })
        }).await
    }

    pub async fn delete_node(&self, id: &str) -> Result<bool, BrainError> {
        let id = id.to_string();
        self.with_conn(move |conn| {
            conn.execute("DELETE FROM nodes WHERE id = ?1", params![id])
                .map_err(|e| BrainError::Database(e.to_string()))?;
            conn.execute(
                "DELETE FROM edges WHERE source_id = ?1 OR target_id = ?1",
                params![id],
            ).map_err(|e| BrainError::Database(e.to_string()))?;
            Ok(true)
        }).await
    }

    pub async fn delete_edge(&self, id: &str) -> Result<bool, BrainError> {
        let id = id.to_string();
        self.with_conn(move |conn| {
            conn.execute("DELETE FROM edges WHERE id = ?1", params![id])
                .map_err(|e| BrainError::Database(e.to_string()))?;
            Ok(true)
        }).await
    }

    /// Full-text search using FTS5 — instant at any scale.
    pub async fn search_nodes(&self, query: &str) -> Result<Vec<SearchResult>, BrainError> {
        let query = query.to_string();
        self.with_conn(move |conn| {
            // Escape FTS5 special characters and build a prefix query
            let fts_query = query
                .split_whitespace()
                .map(|w| format!("\"{}\"*", w.replace('"', "")))
                .collect::<Vec<_>>()
                .join(" ");

            let mut stmt = conn.prepare(
                "SELECT n.id, n.title, n.content, n.summary, n.domain, n.topic, n.tags,
                        n.node_type, n.source_type, n.visual_size, n.access_count,
                        n.decay_score, n.created_at, rank
                 FROM nodes_fts f
                 JOIN nodes n ON n.rowid = f.rowid
                 WHERE nodes_fts MATCH ?1
                 ORDER BY rank
                 LIMIT 50"
            ).map_err(|e| BrainError::Database(e.to_string()))?;

            let rows = stmt.query_map(params![fts_query], |row| {
                let rank: f64 = row.get(13)?;
                Ok(SearchResult {
                    node: GraphNode {
                        id: row.get(0)?,
                        title: row.get(1)?,
                        content: row.get(2)?,
                        summary: row.get(3)?,
                        domain: row.get(4)?,
                        topic: row.get(5)?,
                        tags: parse_tags(&row.get::<_, String>(6).unwrap_or_default()),
                        node_type: row.get(7)?,
                        source_type: row.get(8)?,
                        visual_size: row.get(9)?,
                        access_count: row.get(10)?,
                        decay_score: row.get(11)?,
                        created_at: row.get(12)?,
                    },
                    score: -rank, // FTS5 rank is negative (lower = better)
                    matched_field: "fts5".to_string(),
                })
            }).map_err(|e| BrainError::Database(e.to_string()))?;

            let mut results = Vec::new();
            for row in rows {
                match row {
                    Ok(r) => results.push(r),
                    Err(_) => continue,
                }
            }
            Ok(results)
        }).await
    }

    pub async fn get_brain_stats(&self) -> Result<BrainStats, BrainError> {
        self.with_conn(|conn| {
            let total_nodes: u64 = conn.query_row("SELECT COUNT(*) FROM nodes", [], |row| row.get(0))
                .unwrap_or(0);

            let total_edges: u64 = conn.query_row("SELECT COUNT(*) FROM edges", [], |row| row.get(0))
                .unwrap_or(0);

            // Domain counts
            let mut domain_stmt = conn.prepare(
                "SELECT domain, COUNT(*) as cnt FROM nodes GROUP BY domain ORDER BY cnt DESC"
            ).map_err(|e| BrainError::Database(e.to_string()))?;

            let domains: Vec<DomainCount> = domain_stmt.query_map([], |row| {
                Ok(DomainCount {
                    domain: row.get(0)?,
                    count: row.get(1)?,
                })
            }).map_err(|e| BrainError::Database(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

            // Source type count
            let total_sources: u64 = conn.query_row(
                "SELECT COUNT(DISTINCT source_type) FROM nodes", [], |row| row.get(0)
            ).unwrap_or(0);

            // Recent 10 nodes
            let mut recent_stmt = conn.prepare(
                "SELECT id, title, summary, domain, topic, tags, node_type, source_type,
                        visual_size, access_count, decay_score, created_at
                 FROM nodes ORDER BY created_at DESC LIMIT 10"
            ).map_err(|e| BrainError::Database(e.to_string()))?;

            let recent_nodes: Vec<GraphNode> = recent_stmt.query_map([], |row| {
                Ok(GraphNode {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    content: String::new(),
                    summary: row.get(2)?,
                    domain: row.get(3)?,
                    topic: row.get(4)?,
                    tags: parse_tags(&row.get::<_, String>(5).unwrap_or_default()),
                    node_type: row.get(6)?,
                    source_type: row.get(7)?,
                    visual_size: row.get(8)?,
                    access_count: row.get(9)?,
                    decay_score: row.get(10)?,
                    created_at: row.get(11)?,
                })
            }).map_err(|e| BrainError::Database(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

            Ok(BrainStats {
                total_nodes,
                total_edges,
                domains,
                recent_nodes,
                total_sources,
            })
        }).await
    }

    pub async fn auto_link_nodes(&self) -> Result<AutoLinkResult, BrainError> {
        self.with_conn(|conn| {
            use std::collections::{HashMap, HashSet};

            // Load lightweight node data
            let mut stmt = conn.prepare("SELECT id, domain, topic, tags FROM nodes")
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let all_nodes: Vec<(String, String, String, Vec<String>)> = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    parse_tags(&row.get::<_, String>(3).unwrap_or_default()),
                ))
            }).map_err(|e| BrainError::Database(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

            let total_nodes = all_nodes.len() as u64;

            // Count edges per node
            let mut edge_counts: HashMap<String, u64> = HashMap::new();
            let mut ec_stmt = conn.prepare("SELECT source_id, target_id FROM edges")
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let edge_pairs: Vec<(String, String)> = ec_stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            }).map_err(|e| BrainError::Database(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

            for (src, tgt) in &edge_pairs {
                *edge_counts.entry(src.clone()).or_insert(0) += 1;
                *edge_counts.entry(tgt.clone()).or_insert(0) += 1;
            }

            // Find underlinked nodes
            let underlinked: Vec<usize> = all_nodes.iter().enumerate()
                .filter(|(_, n)| edge_counts.get(&n.0).copied().unwrap_or(0) < 3)
                .map(|(i, _)| i)
                .take(500)
                .collect();

            // Build topic/domain indices
            let mut by_topic: HashMap<String, Vec<usize>> = HashMap::new();
            let mut by_domain: HashMap<String, Vec<usize>> = HashMap::new();
            for (i, n) in all_nodes.iter().enumerate() {
                if !n.2.is_empty() {
                    by_topic.entry(n.2.clone()).or_default().push(i);
                }
                by_domain.entry(n.1.clone()).or_default().push(i);
            }

            // Existing pairs
            let mut existing_pairs: HashSet<(String, String)> = HashSet::new();
            for (src, tgt) in &edge_pairs {
                let (a, b) = if src < tgt { (src.clone(), tgt.clone()) } else { (tgt.clone(), src.clone()) };
                existing_pairs.insert((a, b));
            }

            let mut edges_to_create: Vec<(String, String, f64, String, String)> = Vec::new();
            let mut planned_pairs: HashSet<(String, String)> = HashSet::new();

            for &idx_a in &underlinked {
                let (ref id_a, ref domain_a, ref topic_a, ref tags_a) = all_nodes[idx_a];

                let mut candidates: Vec<usize> = Vec::new();
                if let Some(peers) = by_topic.get(topic_a) {
                    candidates.extend(peers.iter().take(50));
                }
                if let Some(peers) = by_domain.get(domain_a) {
                    for &idx in peers.iter().take(30) {
                        if !candidates.contains(&idx) { candidates.push(idx); }
                    }
                }

                let tags_set_a: HashSet<&str> = tags_a.iter().map(|t| t.as_str()).collect();
                let mut scored: Vec<(usize, f64)> = Vec::new();

                for &j in &candidates {
                    if j == idx_a { continue; }
                    let (ref id_b, ref domain_b, ref topic_b, ref tags_b) = all_nodes[j];
                    if id_a == id_b { continue; }

                    let mut score = 0.0f64;
                    if domain_a == domain_b && topic_a == topic_b { score += 3.0; }
                    let tags_set_b: HashSet<&str> = tags_b.iter().map(|t| t.as_str()).collect();
                    score += tags_set_a.intersection(&tags_set_b).count() as f64;
                    if domain_a == domain_b && score < 3.0 { score += 0.5; }
                    if score > 0.0 { scored.push((j, score)); }
                }

                scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                scored.truncate(3);

                for (j, score) in scored {
                    let (ref id_b, ref domain_b, ref topic_b, _) = all_nodes[j];
                    let (pa, pb) = if id_a < id_b { (id_a.clone(), id_b.clone()) } else { (id_b.clone(), id_a.clone()) };
                    if existing_pairs.contains(&(pa.clone(), pb.clone())) { continue; }
                    if planned_pairs.contains(&(pa.clone(), pb.clone())) { continue; }

                    let (rel, evidence) = if domain_a == domain_b && topic_a == topic_b {
                        ("same_topic".to_string(), format!("Shared topic: {}", topic_a))
                    } else if score >= 1.0 {
                        let shared: Vec<String> = tags_a.iter().filter(|t| all_nodes[j].3.contains(t)).cloned().collect();
                        ("shared_tags".to_string(), format!("Shared tags: {}", shared.join(", ")))
                    } else {
                        ("same_domain".to_string(), format!("Same domain: {}", domain_a))
                    };
                    let strength = if score >= 3.0 { 0.8 } else if score >= 2.0 { 0.6 } else if score >= 1.0 { 0.4 } else { 0.2 };

                    planned_pairs.insert((pa, pb));
                    edges_to_create.push((id_a.clone(), id_b.clone(), strength, rel, evidence));
                }
            }

            // Batch create
            let mut created = 0u64;
            let now = chrono::Utc::now().to_rfc3339();
            for (src, tgt, strength, rel, evidence) in &edges_to_create {
                let eid = format!("edge:{}", Uuid::now_v7());
                match conn.execute(
                    "INSERT INTO edges (id, source_id, target_id, relation_type, strength, discovered_by, evidence, animated, created_at, traversal_count)
                     VALUES (?1, ?2, ?3, ?4, ?5, 'auto_link', ?6, 0, ?7, 0)",
                    params![eid, src, tgt, rel, strength, evidence, now],
                ) {
                    Ok(_) => created += 1,
                    Err(e) => log::warn!("Failed to create auto-link edge: {}", e),
                }
            }

            log::info!("Auto-linked: created {} new synapses ({} underlinked of {} nodes)",
                created, underlinked.len(), total_nodes);

            Ok(AutoLinkResult {
                created,
                existing: existing_pairs.len() as u64,
                total_nodes,
            })
        }).await
    }

    pub async fn get_edges_for_node(&self, node_id: &str) -> Result<Vec<GraphEdge>, BrainError> {
        let node_id = node_id.to_string();
        self.with_conn(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, source_id, target_id, relation_type, strength, animated
                 FROM edges WHERE source_id = ?1 OR target_id = ?1"
            ).map_err(|e| BrainError::Database(e.to_string()))?;

            let rows = stmt.query_map(params![node_id], |row| {
                Ok(GraphEdge {
                    id: row.get(0)?,
                    source: row.get(1)?,
                    target: row.get(2)?,
                    relation_type: row.get(3)?,
                    strength: row.get(4)?,
                    animated: row.get::<_, i32>(5)? != 0,
                })
            }).map_err(|e| BrainError::Database(e.to_string()))?;

            let mut results = Vec::new();
            for row in rows {
                results.push(row.map_err(|e| BrainError::Database(e.to_string()))?);
            }
            Ok(results)
        }).await
    }

    pub async fn update_node_embedding(&self, id: &str, embedding: Vec<f64>) -> Result<(), BrainError> {
        let id = id.to_string();
        let embedding_json = serde_json::to_string(&embedding).unwrap_or_default();
        self.with_conn(move |conn| {
            conn.execute(
                "UPDATE nodes SET embedding = ?1 WHERE id = ?2",
                params![embedding_json, id],
            ).map_err(|e| BrainError::Database(e.to_string()))?;
            Ok(())
        }).await
    }

    /// Semantic vector search — HNSW first, brute-force fallback.
    pub async fn vector_search(&self, query_embedding: Vec<f64>, limit: usize) -> Result<Vec<SearchResult>, BrainError> {
        // Path A: HNSW index
        let hnsw_results = {
            let idx = self.hnsw.read().await;
            if idx.is_empty() { None } else { Some(idx.search(&query_embedding, limit)) }
        };

        if let Some(hits) = hnsw_results {
            if !hits.is_empty() {
                let ids: Vec<String> = hits.iter().map(|(id, _)| id.clone()).collect();
                let sims: Vec<f64> = hits.iter().map(|(_, sim)| *sim).collect();

                return self.with_conn(move |conn| {
                    let mut out: Vec<SearchResult> = Vec::with_capacity(ids.len());
                    for (id_str, sim) in ids.iter().zip(sims.iter()) {
                        let mut stmt = conn.prepare(
                            "SELECT id, title, content, summary, domain, topic, tags, node_type,
                                    source_type, visual_size, access_count, decay_score, created_at
                             FROM nodes WHERE id = ?1"
                        ).map_err(|e| BrainError::Database(e.to_string()))?;

                        if let Ok(node) = stmt.query_row(params![id_str], |row| {
                            Ok(GraphNode {
                                id: row.get(0)?,
                                title: row.get(1)?,
                                content: row.get(2)?,
                                summary: row.get(3)?,
                                domain: row.get(4)?,
                                topic: row.get(5)?,
                                tags: parse_tags(&row.get::<_, String>(6).unwrap_or_default()),
                                node_type: row.get(7)?,
                                source_type: row.get(8)?,
                                visual_size: row.get(9)?,
                                access_count: row.get(10)?,
                                decay_score: row.get(11)?,
                                created_at: row.get(12)?,
                            })
                        }) {
                            out.push(SearchResult { node, score: *sim, matched_field: "hnsw".to_string() });
                        }
                    }
                    Ok(out)
                }).await;
            }
        }

        // Path B: brute-force fallback
        log::debug!("vector_search: HNSW empty, using brute-force fallback");
        self.with_conn(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, title, summary, domain, topic, tags, node_type, source_type,
                        visual_size, access_count, decay_score, created_at, embedding
                 FROM nodes WHERE embedding IS NOT NULL AND embedding != ''
                 LIMIT 20000"
            ).map_err(|e| BrainError::Database(e.to_string()))?;

            let mut scored: Vec<(GraphNode, f64)> = Vec::new();
            let rows = stmt.query_map([], |row| {
                let emb_json: String = row.get(12)?;
                Ok((
                    GraphNode {
                        id: row.get(0)?,
                        title: row.get(1)?,
                        content: String::new(),
                        summary: row.get(2)?,
                        domain: row.get(3)?,
                        topic: row.get(4)?,
                        tags: parse_tags(&row.get::<_, String>(5).unwrap_or_default()),
                        node_type: row.get(6)?,
                        source_type: row.get(7)?,
                        visual_size: row.get(8)?,
                        access_count: row.get(9)?,
                        decay_score: row.get(10)?,
                        created_at: row.get(11)?,
                    },
                    emb_json,
                ))
            }).map_err(|e| BrainError::Database(e.to_string()))?;

            for row in rows {
                if let Ok((node, emb_json)) = row {
                    if let Ok(emb) = serde_json::from_str::<Vec<f64>>(&emb_json) {
                        let sim = crate::embeddings::similarity::cosine_similarity(&query_embedding, &emb);
                        if sim > 0.3 {
                            scored.push((node, sim));
                        }
                    }
                }
            }

            scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            scored.truncate(limit);

            Ok(scored.into_iter().map(|(node, score)| SearchResult {
                node, score, matched_field: "embedding".to_string(),
            }).collect())
        }).await
    }

    pub async fn update_node(
        &self, id: &str, title: Option<String>, content: Option<String>,
        domain: Option<String>, topic: Option<String>, tags: Option<Vec<String>>,
    ) -> Result<GraphNode, BrainError> {
        let id = id.to_string();
        self.with_conn(move |conn| {
            // Read existing
            let mut stmt = conn.prepare(
                "SELECT id, title, content, summary, domain, topic, tags, node_type, source_type,
                        visual_size, access_count, decay_score, created_at
                 FROM nodes WHERE id = ?1"
            ).map_err(|e| BrainError::Database(e.to_string()))?;

            let existing = stmt.query_row(params![id], |row| {
                Ok(GraphNode {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    content: row.get(2)?,
                    summary: row.get(3)?,
                    domain: row.get(4)?,
                    topic: row.get(5)?,
                    tags: parse_tags(&row.get::<_, String>(6).unwrap_or_default()),
                    node_type: row.get(7)?,
                    source_type: row.get(8)?,
                    visual_size: row.get(9)?,
                    access_count: row.get(10)?,
                    decay_score: row.get(11)?,
                    created_at: row.get(12)?,
                })
            }).map_err(|e| BrainError::NotFound(format!("Node not found: {}: {}", id, e)))?;

            let now = chrono::Utc::now().to_rfc3339();
            let new_title = title.unwrap_or(existing.title);
            let new_content = content.unwrap_or(existing.content);
            let new_domain = domain.unwrap_or(existing.domain);
            let new_topic = topic.unwrap_or(existing.topic);
            let new_tags = tags.unwrap_or(existing.tags);
            let tags_json = serde_json::to_string(&new_tags).unwrap_or_else(|_| "[]".to_string());

            conn.execute(
                "UPDATE nodes SET title = ?1, content = ?2, domain = ?3, topic = ?4, tags = ?5,
                                  updated_at = ?6, accessed_at = ?6, access_count = access_count + 1
                 WHERE id = ?7",
                params![new_title, new_content, new_domain, new_topic, tags_json, now, id],
            ).map_err(|e| BrainError::Database(e.to_string()))?;

            Ok(GraphNode {
                id,
                title: new_title,
                content: new_content,
                summary: existing.summary,
                domain: new_domain,
                topic: new_topic,
                tags: new_tags,
                node_type: existing.node_type,
                source_type: existing.source_type,
                visual_size: existing.visual_size,
                access_count: existing.access_count + 1,
                decay_score: existing.decay_score,
                created_at: existing.created_at,
            })
        }).await
    }
}
