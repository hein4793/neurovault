use crate::db::BrainDb;
use crate::db::models::GraphNode;
use crate::error::BrainError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternReport {
    pub hubs: Vec<HubNode>,
    pub bridges: Vec<BridgeNode>,
    pub islands: Vec<GraphNode>,
    pub clusters: Vec<ClusterInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubNode { pub node: GraphNode, pub connection_count: usize }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeNode { pub node: GraphNode, pub connects_domains: Vec<String> }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterInfo { pub domain: String, pub node_count: usize, pub avg_quality: f64 }

struct LightNode {
    id: String, title: String, summary: String, domain: String, topic: String,
    tags: Vec<String>, node_type: String, source_type: String,
    visual_size: f64, access_count: u64, decay_score: f64, quality_score: f64, created_at: String,
}

pub async fn analyze_patterns(db: &BrainDb) -> Result<PatternReport, BrainError> {
    let (nodes, edges) = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, title, summary, domain, topic, tags, node_type, source_type, \
             visual_size, access_count, decay_score, quality_score, created_at FROM nodes"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let nodes: Vec<LightNode> = stmt.query_map([], |row| {
            let tags_json: String = row.get(5)?;
            Ok(LightNode {
                id: row.get(0)?, title: row.get(1)?, summary: row.get(2)?,
                domain: row.get(3)?, topic: row.get(4)?,
                tags: serde_json::from_str(&tags_json).unwrap_or_default(),
                node_type: row.get(6)?, source_type: row.get(7)?,
                visual_size: row.get(8)?, access_count: row.get(9)?,
                decay_score: row.get(10)?, quality_score: row.get(11)?, created_at: row.get(12)?,
            })
        }).map_err(|e| BrainError::Database(e.to_string()))?.filter_map(|r| r.ok()).collect();

        let mut estmt = conn.prepare("SELECT source_id, target_id FROM edges")
            .map_err(|e| BrainError::Database(e.to_string()))?;
        let edges: Vec<(String, String)> = estmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        }).map_err(|e| BrainError::Database(e.to_string()))?.filter_map(|r| r.ok()).collect();

        Ok((nodes, edges))
    }).await?;

    let mut edge_counts: HashMap<String, usize> = HashMap::new();
    let mut neighbors: HashMap<String, Vec<String>> = HashMap::new();
    for (src, tgt) in &edges {
        *edge_counts.entry(src.clone()).or_insert(0) += 1;
        *edge_counts.entry(tgt.clone()).or_insert(0) += 1;
        neighbors.entry(src.clone()).or_default().push(tgt.clone());
        neighbors.entry(tgt.clone()).or_default().push(src.clone());
    }

    let node_map: HashMap<&str, &LightNode> = nodes.iter().map(|n| (n.id.as_str(), n)).collect();

    let to_graph = |n: &LightNode| -> GraphNode {
        GraphNode {
            id: n.id.clone(), title: n.title.clone(), content: String::new(),
            summary: n.summary.clone(), domain: n.domain.clone(), topic: n.topic.clone(),
            tags: n.tags.clone(), node_type: n.node_type.clone(), source_type: n.source_type.clone(),
            visual_size: n.visual_size, access_count: n.access_count,
            decay_score: n.decay_score, created_at: n.created_at.clone(),
        }
    };

    // Hubs
    let mut hub_candidates: Vec<(String, usize)> = edge_counts.iter().map(|(k, v)| (k.clone(), *v)).collect();
    hub_candidates.sort_by(|a, b| b.1.cmp(&a.1));
    let hubs: Vec<HubNode> = hub_candidates.iter().take(10)
        .filter_map(|(id, count)| node_map.get(id.as_str()).map(|n| HubNode { node: to_graph(n), connection_count: *count }))
        .collect();

    // Bridges
    let mut bridges: Vec<BridgeNode> = Vec::new();
    for (id, neighs) in &neighbors {
        if let Some(node) = node_map.get(id.as_str()) {
            let mut domains: std::collections::HashSet<String> = std::collections::HashSet::new();
            domains.insert(node.domain.clone());
            for nid in neighs { if let Some(n) = node_map.get(nid.as_str()) { domains.insert(n.domain.clone()); } }
            if domains.len() >= 3 {
                bridges.push(BridgeNode { node: to_graph(node), connects_domains: domains.into_iter().collect() });
            }
        }
    }
    bridges.sort_by(|a, b| b.connects_domains.len().cmp(&a.connects_domains.len()));
    bridges.truncate(10);

    // Islands
    let islands: Vec<GraphNode> = nodes.iter()
        .filter(|n| edge_counts.get(&n.id).copied().unwrap_or(0) == 0)
        .take(20).map(|n| to_graph(n)).collect();

    // Clusters
    let mut domain_groups: HashMap<String, Vec<&LightNode>> = HashMap::new();
    for node in &nodes { domain_groups.entry(node.domain.clone()).or_default().push(node); }
    let clusters: Vec<ClusterInfo> = domain_groups.iter().map(|(domain, dnodes)| {
        let avg_q = if dnodes.is_empty() { 0.0 } else { dnodes.iter().map(|n| n.quality_score).sum::<f64>() / dnodes.len() as f64 };
        ClusterInfo { domain: domain.clone(), node_count: dnodes.len(), avg_quality: avg_q }
    }).collect();

    Ok(PatternReport { hubs, bridges, islands, clusters })
}
