use crate::db::models::GraphNode;
use crate::db::BrainDb;
use crate::error::BrainError;
use rusqlite::params;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct DuplicatePair {
    pub node_a: GraphNode,
    pub node_b: GraphNode,
    pub similarity: f64,
    pub recommendation: String,
}

/// Merge two nodes: keep the first, absorb content/tags/edges from the second, then delete the second.
pub async fn merge_nodes(db: &BrainDb, keep_id: &str, remove_id: &str) -> Result<GraphNode, BrainError> {
    let keep = keep_id.to_string();
    let remove = remove_id.to_string();

    let node = db.with_conn(move |conn| {
        // Load both nodes
        let keep_node = load_node(conn, &keep)?;
        let remove_node = load_node(conn, &remove)?;

        // Merge content
        let merged_content = format!("{}\n\n---\n\n{}", keep_node.content, remove_node.content);

        // Union of tags
        let keep_tags: Vec<String> = serde_json::from_str(&keep_node.tags_json).unwrap_or_default();
        let remove_tags: Vec<String> = serde_json::from_str(&remove_node.tags_json).unwrap_or_default();
        let mut merged_tags = keep_tags.clone();
        for tag in &remove_tags {
            if !merged_tags.contains(tag) {
                merged_tags.push(tag.clone());
            }
        }

        let merged_access = keep_node.access_count + remove_node.access_count;
        let now = chrono::Utc::now().to_rfc3339();
        let tags_json = serde_json::to_string(&merged_tags).unwrap_or_else(|_| "[]".to_string());
        let max_quality = keep_node.quality_score.max(remove_node.quality_score);

        // Update the kept node
        conn.execute(
            "UPDATE nodes SET content = ?1, tags = ?2, access_count = ?3, \
             updated_at = ?4, quality_score = ?5 WHERE id = ?6",
            params![merged_content, tags_json, merged_access, now, max_quality, keep],
        ).map_err(|e| BrainError::Database(e.to_string()))?;

        // Redirect edges from removed node to kept node
        conn.execute(
            "UPDATE edges SET source_id = ?1 WHERE source_id = ?2",
            params![keep, remove],
        ).map_err(|e| BrainError::Database(e.to_string()))?;

        conn.execute(
            "UPDATE edges SET target_id = ?1 WHERE target_id = ?2",
            params![keep, remove],
        ).map_err(|e| BrainError::Database(e.to_string()))?;

        // Delete the removed node
        conn.execute("DELETE FROM nodes WHERE id = ?1", params![remove])
            .map_err(|e| BrainError::Database(e.to_string()))?;

        // Return the updated node
        let mut stmt = conn.prepare(
            "SELECT id, title, content, summary, domain, topic, tags, node_type, \
             source_type, visual_size, access_count, decay_score, created_at \
             FROM nodes WHERE id = ?1"
        ).map_err(|e| BrainError::Database(e.to_string()))?;

        stmt.query_row(params![keep], |row| {
            let tags_str: String = row.get(6)?;
            Ok(GraphNode {
                id: row.get(0)?,
                title: row.get(1)?,
                content: row.get(2)?,
                summary: row.get(3)?,
                domain: row.get(4)?,
                topic: row.get(5)?,
                tags: serde_json::from_str(&tags_str).unwrap_or_default(),
                node_type: row.get(7)?,
                source_type: row.get(8)?,
                visual_size: row.get(9)?,
                access_count: row.get(10)?,
                decay_score: row.get(11)?,
                created_at: row.get(12)?,
            })
        }).map_err(|e| BrainError::Database(e.to_string()))
    }).await?;

    Ok(node)
}

struct NodeRow {
    content: String,
    tags_json: String,
    access_count: u64,
    quality_score: f64,
}

fn load_node(conn: &rusqlite::Connection, id: &str) -> Result<NodeRow, BrainError> {
    conn.query_row(
        "SELECT content, tags, access_count, quality_score FROM nodes WHERE id = ?1",
        params![id],
        |row| Ok(NodeRow {
            content: row.get(0)?,
            tags_json: row.get(1)?,
            access_count: row.get(2)?,
            quality_score: row.get(3)?,
        }),
    ).map_err(|e| BrainError::NotFound(format!("Node not found: {} ({})", id, e)))
}
