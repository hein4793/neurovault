use crate::db::models::GraphNode;
use crate::db::BrainDb;
use crate::error::BrainError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimilarNode { pub node: GraphNode, pub similarity: f64 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicatePair { pub node_a: GraphNode, pub node_b: GraphNode, pub similarity: f64, pub recommendation: String }

struct NodeWithEmbedding {
    id: String, title: String, content: String, summary: String, domain: String,
    topic: String, tags: Vec<String>, node_type: String, source_type: String,
    visual_size: f64, access_count: u64, decay_score: f64, created_at: String,
    embedding: Vec<f64>,
}

impl NodeWithEmbedding {
    fn to_graph_node(&self) -> GraphNode {
        GraphNode {
            id: self.id.clone(), title: self.title.clone(), content: self.content.clone(),
            summary: self.summary.clone(), domain: self.domain.clone(), topic: self.topic.clone(),
            tags: self.tags.clone(), node_type: self.node_type.clone(), source_type: self.source_type.clone(),
            visual_size: self.visual_size, access_count: self.access_count,
            decay_score: self.decay_score, created_at: self.created_at.clone(),
        }
    }
}

pub fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
    if a.len() != b.len() || a.is_empty() { return 0.0; }
    let mut dot = 0.0f64; let mut na = 0.0f64; let mut nb = 0.0f64;
    for i in 0..a.len() { dot += a[i] * b[i]; na += a[i] * a[i]; nb += b[i] * b[i]; }
    let denom = na.sqrt() * nb.sqrt();
    if denom == 0.0 { 0.0 } else { dot / denom }
}

async fn load_nodes_with_embeddings(db: &BrainDb) -> Result<Vec<NodeWithEmbedding>, BrainError> {
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT n.id, n.title, n.content, n.summary, n.domain, n.topic, n.tags, n.node_type, \
             n.source_type, n.visual_size, n.access_count, n.decay_score, n.created_at, \
             e.vector, e.dimension \
             FROM nodes n INNER JOIN embeddings e ON e.node_id = n.id"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            let tags_json: String = row.get(6)?;
            let blob: Vec<u8> = row.get(13)?;
            let dim: usize = row.get(14)?;
            let emb: Vec<f64> = blob.chunks_exact(8).take(dim)
                .map(|c| f64::from_le_bytes(c.try_into().unwrap_or([0u8; 8]))).collect();
            Ok(NodeWithEmbedding {
                id: row.get(0)?, title: row.get(1)?, content: row.get(2)?, summary: row.get(3)?,
                domain: row.get(4)?, topic: row.get(5)?,
                tags: serde_json::from_str(&tags_json).unwrap_or_default(),
                node_type: row.get(7)?, source_type: row.get(8)?,
                visual_size: row.get(9)?, access_count: row.get(10)?,
                decay_score: row.get(11)?, created_at: row.get(12)?, embedding: emb,
            })
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(n) = r { result.push(n); } }
        Ok(result)
    }).await
}

pub async fn find_similar(db: &BrainDb, node_id: &str, threshold: f64, limit: usize) -> Result<Vec<SimilarNode>, BrainError> {
    let nodes = load_nodes_with_embeddings(db).await?;
    let target = nodes.iter().find(|n| n.id == node_id)
        .ok_or_else(|| BrainError::NotFound(format!("Node {} not found or has no embedding", node_id)))?;
    let target_emb = &target.embedding;
    let target_id = &target.id;

    let mut results: Vec<SimilarNode> = nodes.iter()
        .filter(|c| c.id != *target_id)
        .filter_map(|c| {
            let sim = cosine_similarity(target_emb, &c.embedding);
            if sim >= threshold { Some(SimilarNode { node: c.to_graph_node(), similarity: sim }) } else { None }
        }).collect();
    results.sort_by(|a, b| b.similarity.partial_cmp(&a.similarity).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(limit);
    Ok(results)
}

pub async fn detect_duplicates(db: &BrainDb, threshold: f64) -> Result<Vec<DuplicatePair>, BrainError> {
    let nodes = load_nodes_with_embeddings(db).await?;
    let mut pairs: Vec<DuplicatePair> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for i in 0..nodes.len() {
        for j in (i + 1)..nodes.len() {
            let sim = cosine_similarity(&nodes[i].embedding, &nodes[j].embedding);
            if sim >= threshold {
                let pair_key = if nodes[i].id < nodes[j].id { format!("{}:{}", nodes[i].id, nodes[j].id) }
                    else { format!("{}:{}", nodes[j].id, nodes[i].id) };
                if seen.contains(&pair_key) { continue; }
                seen.insert(pair_key);
                let rec = if sim >= 0.95 { "merge" } else if sim >= 0.92 { "review" } else { "related" };
                pairs.push(DuplicatePair {
                    node_a: nodes[i].to_graph_node(), node_b: nodes[j].to_graph_node(),
                    similarity: sim, recommendation: rec.to_string(),
                });
            }
        }
    }
    pairs.sort_by(|a, b| b.similarity.partial_cmp(&a.similarity).unwrap_or(std::cmp::Ordering::Equal));
    Ok(pairs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn test_identical() { assert!((cosine_similarity(&[1.0, 2.0, 3.0], &[1.0, 2.0, 3.0]) - 1.0).abs() < 1e-10); }
    #[test] fn test_orthogonal() { assert!(cosine_similarity(&[1.0, 0.0], &[0.0, 1.0]).abs() < 1e-10); }
    #[test] fn test_opposite() { assert!((cosine_similarity(&[1.0, 2.0, 3.0], &[-1.0, -2.0, -3.0]) + 1.0).abs() < 1e-10); }
    #[test] fn test_empty() { assert_eq!(cosine_similarity(&[], &[]), 0.0); }
    #[test] fn test_mismatched() { assert_eq!(cosine_similarity(&[1.0, 2.0], &[1.0, 2.0, 3.0]), 0.0); }
    #[test] fn test_zero() { assert_eq!(cosine_similarity(&[0.0, 0.0, 0.0], &[1.0, 2.0, 3.0]), 0.0); }
}
