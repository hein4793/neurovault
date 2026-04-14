use crate::db::BrainDb;
use crate::error::BrainError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    pub rec_type: String,
    pub title: String,
    pub description: String,
    pub priority: f64,
    pub node_id: Option<String>,
}

pub async fn get_recommendations(db: &BrainDb) -> Result<Vec<Recommendation>, BrainError> {
    let (nodes, edge_counts) = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, title, node_type, quality_score, LENGTH(content), updated_at FROM nodes"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let nodes: Vec<(String, String, String, f64, u64, String)> = stmt.query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?))
        }).map_err(|e| BrainError::Database(e.to_string()))?.filter_map(|r| r.ok()).collect();

        let mut counts: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
        let mut s1 = conn.prepare("SELECT source_id, COUNT(*) FROM edges GROUP BY source_id")
            .map_err(|e| BrainError::Database(e.to_string()))?;
        for r in s1.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?)))
            .map_err(|e| BrainError::Database(e.to_string()))? {
            if let Ok((id, c)) = r { *counts.entry(id).or_insert(0) += c; }
        }
        let mut s2 = conn.prepare("SELECT target_id, COUNT(*) FROM edges GROUP BY target_id")
            .map_err(|e| BrainError::Database(e.to_string()))?;
        for r in s2.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?)))
            .map_err(|e| BrainError::Database(e.to_string()))? {
            if let Ok((id, c)) = r { *counts.entry(id).or_insert(0) += c; }
        }

        Ok((nodes, counts))
    }).await?;

    let mut recs: Vec<Recommendation> = Vec::new();
    let now = chrono::Utc::now();

    let mut connect_count = 0;
    for (id, title, node_type, _, _, _) in &nodes {
        if connect_count >= 10 { break; }
        let count = edge_counts.get(id).copied().unwrap_or(0);
        if count == 0 && node_type != "conversation" {
            recs.push(Recommendation {
                rec_type: "connect".to_string(),
                title: format!("Connect: {}", title),
                description: "This node has no connections. Run auto-link or connect it manually.".to_string(),
                priority: 0.7, node_id: Some(id.clone()),
            });
            connect_count += 1;
        }
    }

    let mut stale_count = 0;
    for (id, title, _, quality, _, updated_at) in &nodes {
        if stale_count >= 5 { break; }
        if let Ok(updated) = chrono::DateTime::parse_from_rfc3339(updated_at) {
            let days = (now - updated.with_timezone(&chrono::Utc)).num_days();
            if days > 60 && *quality > 0.5 {
                recs.push(Recommendation {
                    rec_type: "update".to_string(),
                    title: format!("Update: {}", title),
                    description: format!("Not updated in {} days. Content may be stale.", days),
                    priority: 0.5, node_id: Some(id.clone()),
                });
                stale_count += 1;
            }
        }
    }

    let mut low_count = 0;
    for (id, title, _, quality, content_len, _) in &nodes {
        if low_count >= 5 { break; }
        if *quality < 0.2 && *content_len < 100 {
            recs.push(Recommendation {
                rec_type: "update".to_string(),
                title: format!("Improve: {}", title),
                description: "Low quality score and short content. Consider enriching.".to_string(),
                priority: 0.4, node_id: Some(id.clone()),
            });
            low_count += 1;
        }
    }

    recs.sort_by(|a, b| b.priority.partial_cmp(&a.priority).unwrap_or(std::cmp::Ordering::Equal));
    recs.truncate(20);
    Ok(recs)
}
