use crate::db::models::*;
use crate::db::BrainDb;
use crate::error::BrainError;
use crate::events::{emit_event, BrainEvent};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::State;

/// Flat arrays for instanced point cloud rendering of ALL nodes.
/// Much lighter than sending 198K JSON objects — just positions + colors + sizes.
#[derive(Debug, Clone, Serialize)]
pub struct NodeCloud {
    pub positions: Vec<f32>,  // [x,y,z, x,y,z, ...] — 3 floats per node
    pub colors: Vec<f32>,     // [r,g,b, r,g,b, ...] — 3 floats per node (0-1 range)
    pub sizes: Vec<f32>,      // [s, s, ...] — 1 float per node
    pub count: u64,
}

#[tauri::command]
pub async fn get_all_nodes(db: State<'_, Arc<BrainDb>>) -> Result<Vec<GraphNode>, BrainError> {
    db.get_all_nodes().await
}

#[tauri::command]
pub async fn get_all_edges(db: State<'_, Arc<BrainDb>>) -> Result<Vec<GraphEdge>, BrainError> {
    db.get_all_edges().await
}

#[tauri::command]
pub async fn create_node(
    app: tauri::AppHandle,
    db: State<'_, Arc<BrainDb>>,
    title: String,
    content: String,
    domain: String,
    topic: String,
    tags: Vec<String>,
    node_type: String,
    source_type: String,
    source_url: Option<String>,
) -> Result<GraphNode, BrainError> {
    let node = db
        .create_node(CreateNodeInput {
            title,
            content,
            domain,
            topic,
            tags,
            node_type,
            source_type,
            source_url,
        })
        .await?;

    emit_event(
        &app,
        BrainEvent::NodeCreated {
            id: node.id.clone(),
            title: node.title.clone(),
        },
    );

    Ok(node)
}

#[tauri::command]
pub async fn update_node(
    app: tauri::AppHandle,
    db: State<'_, Arc<BrainDb>>,
    id: String,
    title: Option<String>,
    content: Option<String>,
    domain: Option<String>,
    topic: Option<String>,
    tags: Option<Vec<String>>,
) -> Result<GraphNode, BrainError> {
    let node = db
        .update_node(&id, title, content, domain, topic, tags)
        .await?;

    emit_event(
        &app,
        BrainEvent::NodeUpdated {
            id: node.id.clone(),
            title: node.title.clone(),
        },
    );

    Ok(node)
}

#[tauri::command]
pub async fn delete_node(
    app: tauri::AppHandle,
    db: State<'_, Arc<BrainDb>>,
    id: String,
) -> Result<bool, BrainError> {
    let result = db.delete_node(&id).await?;

    emit_event(&app, BrainEvent::NodeDeleted { id });

    Ok(result)
}

#[tauri::command]
pub async fn create_edge(
    app: tauri::AppHandle,
    db: State<'_, Arc<BrainDb>>,
    source_id: String,
    target_id: String,
    relation_type: String,
    evidence: String,
) -> Result<GraphEdge, BrainError> {
    let edge = db
        .create_edge(CreateEdgeInput {
            source_id: source_id.clone(),
            target_id: target_id.clone(),
            relation_type,
            evidence,
        })
        .await?;

    emit_event(
        &app,
        BrainEvent::EdgeCreated {
            id: edge.id.clone(),
            source: source_id,
            target: target_id,
        },
    );

    Ok(edge)
}

#[tauri::command]
pub async fn delete_edge(
    app: tauri::AppHandle,
    db: State<'_, Arc<BrainDb>>,
    id: String,
) -> Result<bool, BrainError> {
    let result = db.delete_edge(&id).await?;

    emit_event(&app, BrainEvent::EdgeDeleted { id });

    Ok(result)
}

#[tauri::command]
pub async fn get_node_count(db: State<'_, Arc<BrainDb>>) -> Result<u64, BrainError> {
    db.get_node_count().await
}

/// Phase 4 follow-up — fast edge count for the StatusBar synapse counter.
/// Avoids the slow `get_brain_stats` GROUP BY chain that was causing the
/// status bar to show 0 synapses indefinitely on large brains.
#[tauri::command]
pub async fn get_edge_count(db: State<'_, Arc<BrainDb>>) -> Result<u64, BrainError> {
    db.get_edge_count().await
}

#[tauri::command]
pub async fn get_nodes_paginated(
    db: State<'_, Arc<BrainDb>>,
    offset: u64,
    limit: u64,
) -> Result<Vec<GraphNode>, BrainError> {
    db.get_nodes_paginated(offset, limit).await
}

#[tauri::command]
pub async fn get_edges_for_node(
    db: State<'_, Arc<BrainDb>>,
    node_id: String,
) -> Result<Vec<GraphEdge>, BrainError> {
    db.get_edges_for_node(&node_id).await
}

/// Get ALL nodes as a lightweight point cloud for instanced rendering.
/// Uses three importance tiers: top 1000 by access_count (large), next 10000 (medium),
/// rest generated synthetically from domain counts (small).
#[tauri::command]
pub async fn get_node_cloud(
    db: State<'_, Arc<BrainDb>>,
) -> Result<NodeCloud, BrainError> {
    #[derive(Debug)]
    #[allow(dead_code)]
    struct TierNodeRow {
        domain: Option<String>,
        quality_score: Option<f64>,
        access_count: Option<u64>,
    }

    #[derive(Debug)]
    struct DomainRow {
        domain: String,
        count: u64,
    }

    // Fetch tier 1, tier 2, and domain counts in one with_conn call
    let (tier1_nodes, tier2_nodes, domain_counts) = db.with_conn(|conn| -> Result<(Vec<TierNodeRow>, Vec<TierNodeRow>, Vec<DomainRow>), BrainError> {
        // --- Tier 1: First 1000 nodes (large dots) ---
        let mut stmt1 = conn.prepare(
            "SELECT domain, quality_score, access_count FROM nodes LIMIT 1000"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let tier1: Vec<TierNodeRow> = stmt1.query_map([], |row| {
            Ok(TierNodeRow {
                domain: row.get(0)?,
                quality_score: row.get(1)?,
                access_count: row.get::<_, Option<i64>>(2)?.map(|v| v as u64),
            })
        }).map_err(|e| BrainError::Database(e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();

        // --- Tier 2: Next 10000 nodes (medium dots) ---
        let mut stmt2 = conn.prepare(
            "SELECT domain, quality_score, access_count FROM nodes LIMIT 10000 OFFSET 1000"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let tier2: Vec<TierNodeRow> = stmt2.query_map([], |row| {
            Ok(TierNodeRow {
                domain: row.get(0)?,
                quality_score: row.get(1)?,
                access_count: row.get::<_, Option<i64>>(2)?.map(|v| v as u64),
            })
        }).map_err(|e| BrainError::Database(e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();

        // --- Domain counts ---
        let mut stmt3 = conn.prepare(
            "SELECT domain, COUNT(*) as count FROM nodes GROUP BY domain"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let domains: Vec<DomainRow> = stmt3.query_map([], |row| {
            Ok(DomainRow {
                domain: row.get(0)?,
                count: row.get::<_, i64>(1)? as u64,
            })
        }).map_err(|e| BrainError::Database(e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();

        Ok((tier1, tier2, domains))
    }).await?;

    // Count tier 1+2 nodes per domain to subtract from totals
    let mut tier12_per_domain: HashMap<String, u64> = HashMap::new();
    for n in &tier1_nodes {
        let d = n.domain.clone().unwrap_or_default();
        *tier12_per_domain.entry(d).or_insert(0) += 1;
    }
    for n in &tier2_nodes {
        let d = n.domain.clone().unwrap_or_default();
        *tier12_per_domain.entry(d).or_insert(0) += 1;
    }

    let tier3_total: u64 = domain_counts.iter().map(|d| {
        let used = tier12_per_domain.get(&d.domain).copied().unwrap_or(0);
        d.count.saturating_sub(used)
    }).sum();

    let total = tier1_nodes.len() as u64 + tier2_nodes.len() as u64 + tier3_total;
    if total == 0 {
        return Ok(NodeCloud { positions: Vec::new(), colors: Vec::new(), sizes: Vec::new(), count: 0 });
    }

    let brain_scale: f32 = 180.0;
    let capacity = total as usize;
    let mut positions: Vec<f32> = Vec::with_capacity(capacity * 3);
    let mut colors: Vec<f32> = Vec::with_capacity(capacity * 3);
    let mut sizes: Vec<f32> = Vec::with_capacity(capacity);

    // Helper: domain -> VERY distinct colors (easily distinguishable)
    fn domain_color(domain: &str) -> (f32, f32, f32) {
        match domain {
            "technology" => (0.2, 0.6, 1.0),    // electric blue
            "business"   => (0.0, 1.0, 0.5),    // neon green
            "research"   => (0.7, 0.3, 1.0),    // vivid purple
            "pattern"    => (1.0, 0.8, 0.0),    // gold/yellow
            "reference"  => (0.0, 0.9, 0.9),    // cyan/teal
            "personal"   => (1.0, 0.4, 0.2),    // bright orange
            _            => (0.5, 0.5, 1.0),    // soft blue
        }
    }

    // Helper: domain -> seed offset for deterministic positions
    fn domain_seed(domain: &str) -> u64 {
        match domain {
            "technology" => 0,
            "business"   => 100000,
            "personal"   => 200000,
            "research"   => 300000,
            "pattern"    => 400000,
            "reference"  => 500000,
            _            => 600000,
        }
    }

    // Helper: generate brain-shaped position from a seed index.
    fn brain_position(idx: u64, brain_scale: f32) -> (f32, f32, f32) {
        let r = rand_seeded(idx * 3).cbrt();
        let theta = rand_seeded(idx * 3 + 1) * std::f32::consts::PI * 2.0;
        let phi = (2.0 * rand_seeded(idx * 3 + 2) - 1.0).acos();

        let rx = brain_scale * 1.45;
        let ry = brain_scale * 1.10;
        let rz = brain_scale * 1.25;

        let mut x = r * phi.sin() * theta.cos() * rx;
        let y_raw = r * phi.sin() * theta.sin() * ry;
        let z = r * phi.cos() * rz;

        if x > 0.0 { x += 6.0; }
        if x < 0.0 { x -= 6.0; }

        let mut y = y_raw + brain_scale * 0.15;

        if y < -brain_scale * 0.1 {
            y = -brain_scale * 0.1 + (y + brain_scale * 0.1) * 0.25;
        }

        if z > 0.0 {
            let z_factor = z / (brain_scale * 1.25);
            let front_extend = 1.0 + z_factor * 0.08;
            let _ = front_extend;
        }

        (x, y, z)
    }

    // Per-domain counters for seed indexing across tiers
    let mut domain_counters: HashMap<String, u64> = HashMap::new();

    // --- Emit Tier 1 points (large: 3.0-5.0, quality-scaled) ---
    for node in &tier1_nodes {
        let d = node.domain.as_deref().unwrap_or("unknown");
        let (cr, cg, cb) = domain_color(d);
        let counter = domain_counters.entry(d.to_string()).or_insert(0);
        let idx = domain_seed(d) + *counter;
        *counter += 1;

        let (x, y, z) = brain_position(idx, brain_scale);
        let q = node.quality_score.unwrap_or(0.5) as f32;
        let size = 5.0 + q * 4.0;

        positions.push(x);
        positions.push(y);
        positions.push(z);
        colors.push(cr);
        colors.push(cg);
        colors.push(cb);
        sizes.push(size);
    }

    // --- Emit Tier 2 points (medium: 1.0-2.0, quality-scaled) ---
    for node in &tier2_nodes {
        let d = node.domain.as_deref().unwrap_or("unknown");
        let (cr, cg, cb) = domain_color(d);
        let counter = domain_counters.entry(d.to_string()).or_insert(0);
        let idx = domain_seed(d) + *counter;
        *counter += 1;

        let (x, y, z) = brain_position(idx, brain_scale);
        let q = node.quality_score.unwrap_or(0.5) as f32;
        let size = 2.0 + q * 2.0;

        positions.push(x);
        positions.push(y);
        positions.push(z);
        colors.push(cr);
        colors.push(cg);
        colors.push(cb);
        sizes.push(size);
    }

    // --- Emit Tier 3 points (small: 0.3-0.8, synthetic from domain counts) ---
    for domain_row in &domain_counts {
        let used = tier12_per_domain.get(&domain_row.domain).copied().unwrap_or(0);
        let remaining = domain_row.count.saturating_sub(used);
        if remaining == 0 { continue; }

        let d = domain_row.domain.as_str();
        let (cr, cg, cb) = domain_color(d);
        let counter = domain_counters.entry(d.to_string()).or_insert(0);

        for _ in 0..remaining {
            let idx = domain_seed(d) + *counter;
            *counter += 1;

            let (x, y, z) = brain_position(idx, brain_scale);
            let size = 0.8 + rand_seeded(idx * 7) * 1.2;

            positions.push(x);
            positions.push(y);
            positions.push(z);
            colors.push(cr);
            colors.push(cg);
            colors.push(cb);
            sizes.push(size);
        }
    }

    log::info!("Node cloud: {} points (tier1={}, tier2={}, tier3={})",
        total, tier1_nodes.len(), tier2_nodes.len(), tier3_total);
    Ok(NodeCloud { positions, colors, sizes, count: total })
}

/// Deterministic hash-based random from a seed index.
fn rand_seeded(seed: u64) -> f32 {
    let mut s = seed.wrapping_mul(2654435761);
    s ^= s >> 16;
    s = s.wrapping_mul(0x45d9f3b);
    s ^= s >> 16;
    ((s & 0xFFFFFF) as f32) / (0xFFFFFF as f32)
}

// --- Domain Clusters ---

#[derive(Debug, Clone, Serialize)]
pub struct DomainCluster {
    pub domain: String,
    pub node_count: u64,
    pub avg_quality: f64,
    pub position: [f32; 3],
    pub color: [f32; 3],
}

/// Get domain cluster summaries with pre-assigned brain-region positions.
#[tauri::command]
pub async fn get_domain_clusters(
    db: State<'_, Arc<BrainDb>>,
) -> Result<Vec<DomainCluster>, BrainError> {
    let mut rows = db.with_conn(|conn| -> Result<Vec<(String, u64, f64)>, BrainError> {
        let mut stmt = conn.prepare(
            "SELECT domain, COUNT(*) as count, AVG(quality_score) as avg_quality \
             FROM nodes GROUP BY domain"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let mapped = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)? as u64,
                row.get::<_, f64>(2).unwrap_or(0.0),
            ))
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in mapped { if let Ok(v) = r { result.push(v); } }
        Ok(result)
    }).await?;

    // Sort by count descending
    rows.sort_by(|a, b| b.1.cmp(&a.1));

    let clusters: Vec<DomainCluster> = rows.into_iter().map(|(domain, count, avg_quality)| {
        let (pos, color) = match domain.as_str() {
            "technology" => ([100.0_f32, 40.0, 30.0],   [0.0_f32, 0.66, 1.0]),
            "business"   => ([-100.0, 40.0, 30.0],      [0.0, 0.8, 0.53]),
            "personal"   => ([70.0, -20.0, -50.0],      [0.98, 0.45, 0.09]),
            "research"   => ([0.0, 90.0, 0.0],          [0.55, 0.36, 0.96]),
            "pattern"    => ([-70.0, -20.0, -50.0],     [0.96, 0.62, 0.04]),
            "reference"  => ([0.0, -50.0, 0.0],         [0.58, 0.64, 0.66]),
            _            => ([0.0, 0.0, 60.0],          [0.22, 0.74, 0.97]),
        };
        DomainCluster {
            domain,
            node_count: count,
            avg_quality,
            position: pos,
            color,
        }
    }).collect();

    log::info!("Domain clusters: {} domains returned", clusters.len());
    Ok(clusters)
}

// --- Edge Bundle Counts ---

#[derive(Debug, Clone, Serialize)]
pub struct EdgeBundle {
    pub source_domain: String,
    pub target_domain: String,
    pub count: u64,
}

/// Get cross-domain edge bundle counts for visualization.
/// Samples up to 10000 edges, resolves node domains, and aggregates by domain pair.
#[tauri::command]
pub async fn get_edge_bundle_counts(
    db: State<'_, Arc<BrainDb>>,
) -> Result<Vec<EdgeBundle>, BrainError> {
    let bundles = db.with_conn(|conn| -> Result<Vec<EdgeBundle>, BrainError> {
        // Step 1: Load a sample of edges
        let mut stmt = conn.prepare(
            "SELECT source_id, target_id FROM edges LIMIT 10000"
        ).map_err(|e| BrainError::Database(e.to_string()))?;

        struct EdgeRow {
            source_id: String,
            target_id: String,
        }

        let edges: Vec<EdgeRow> = stmt.query_map([], |row| {
            Ok(EdgeRow {
                source_id: row.get(0)?,
                target_id: row.get(1)?,
            })
        }).map_err(|e| BrainError::Database(e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();

        if edges.is_empty() {
            return Ok(Vec::new());
        }

        // Step 2: Collect unique node IDs
        let mut node_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
        for e in &edges {
            node_ids.insert(e.source_id.clone());
            node_ids.insert(e.target_id.clone());
        }

        // Step 3: Batch-query domains for these node IDs
        let mut domain_map: HashMap<String, String> = HashMap::new();
        let id_vec: Vec<String> = node_ids.into_iter().collect();
        for chunk in id_vec.chunks(2000) {
            let placeholders: String = chunk.iter().enumerate()
                .map(|(i, _)| format!("?{}", i + 1))
                .collect::<Vec<_>>()
                .join(", ");
            let query = format!("SELECT id, domain FROM nodes WHERE id IN ({})", placeholders);
            let mut stmt = conn.prepare(&query)
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let params: Vec<&dyn rusqlite::types::ToSql> = chunk.iter()
                .map(|s| s as &dyn rusqlite::types::ToSql)
                .collect();
            let rows = stmt.query_map(params.as_slice(), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            }).map_err(|e| BrainError::Database(e.to_string()))?;
            for r in rows {
                if let Ok((id, domain)) = r {
                    domain_map.insert(id, domain);
                }
            }
        }

        // Step 4: Count domain pairs
        let mut pair_counts: HashMap<(String, String), u64> = HashMap::new();
        for e in &edges {
            let src_domain = domain_map.get(&e.source_id).cloned().unwrap_or_default();
            let tgt_domain = domain_map.get(&e.target_id).cloned().unwrap_or_default();
            if src_domain.is_empty() || tgt_domain.is_empty() { continue; }
            let key = if src_domain <= tgt_domain {
                (src_domain, tgt_domain)
            } else {
                (tgt_domain, src_domain)
            };
            *pair_counts.entry(key).or_insert(0) += 1;
        }

        // Step 5: Convert to sorted vec
        let mut bundles: Vec<EdgeBundle> = pair_counts.into_iter().map(|((s, t), c)| {
            EdgeBundle { source_domain: s, target_domain: t, count: c }
        }).collect();
        bundles.sort_by(|a, b| b.count.cmp(&a.count));

        Ok(bundles)
    }).await?;

    log::info!("Edge bundles: {} domain pairs", bundles.len());
    Ok(bundles)
}
