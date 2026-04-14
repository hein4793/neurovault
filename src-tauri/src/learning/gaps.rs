use crate::db::BrainDb;
use crate::error::BrainError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeGap {
    pub topic: String,
    pub reason: String,
    pub priority: f64,
    pub domain: String,
}

/// Detect knowledge gaps by analyzing domain distribution and topic coverage
pub async fn detect_gaps(db: &BrainDb) -> Result<Vec<KnowledgeGap>, BrainError> {
    // Use aggregate queries instead of loading all 198K nodes
    let mut gaps: Vec<KnowledgeGap> = Vec::new();

    // 1. Domain imbalance via aggregate
    struct DomainRow { domain: String, count: u64 }
    let domain_rows: Vec<DomainRow> = db.with_conn(|conn| -> Result<Vec<DomainRow>, BrainError> {
        let mut stmt = conn.prepare(
            "SELECT domain, COUNT(*) AS count FROM nodes GROUP BY domain"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok(DomainRow { domain: row.get(0)?, count: row.get(1)? })
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(n) = r { result.push(n); } }
        Ok(result)
    }).await?;

    let mut domain_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for r in &domain_rows {
        domain_counts.insert(r.domain.clone(), r.count as usize);
    }

    let expected_domains = vec!["technology", "business", "pattern", "research", "reference"];
    let max_count = domain_counts.values().max().copied().unwrap_or(1);

    for domain in &expected_domains {
        let count = domain_counts.get(*domain).copied().unwrap_or(0);
        if count == 0 || (count as f64) < (max_count as f64 * 0.1) {
            gaps.push(KnowledgeGap {
                topic: domain.to_string(),
                reason: format!("Domain '{}' has only {} nodes ({}% of largest domain)", domain, count, if max_count > 0 { count * 100 / max_count } else { 0 }),
                priority: 0.8 - (count as f64 / max_count.max(1) as f64),
                domain: domain.to_string(),
            });
        }
    }

    // 2. Dead-end nodes: count via aggregate
    // Total nodes
    let total_nodes: u64 = db.with_conn(|conn| -> Result<u64, BrainError> {
        let count: u64 = conn.query_row(
            "SELECT COUNT(*) FROM nodes", [], |row| row.get(0)
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(count)
    }).await?;

    // Nodes that appear in any edge
    let connected_count: u64 = db.with_conn(|conn| -> Result<u64, BrainError> {
        let count: u64 = conn.query_row(
            "SELECT COUNT(DISTINCT nid) FROM (\
                SELECT source_id AS nid FROM edges \
                UNION \
                SELECT target_id AS nid FROM edges\
            )", [], |row| row.get(0)
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(count)
    }).await?;

    let isolated_count = total_nodes.saturating_sub(connected_count);

    if isolated_count > 5 {
        gaps.push(KnowledgeGap {
            topic: "isolated_nodes".to_string(),
            reason: format!("{} nodes have no connections - they need to be linked to related knowledge", isolated_count),
            priority: 0.6,
            domain: "reference".to_string(),
        });
    }

    // 3. Topic coverage via aggregate
    struct TopicRow { topic: String, count: u64, latest: String }
    let topic_rows: Vec<TopicRow> = db.with_conn(|conn| -> Result<Vec<TopicRow>, BrainError> {
        let mut stmt = conn.prepare(
            "SELECT topic, COUNT(*) AS count, MAX(created_at) AS latest \
             FROM nodes WHERE topic != '' GROUP BY topic LIMIT 50"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok(TopicRow { topic: row.get(0)?, count: row.get(1)?, latest: row.get(2)? })
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(n) = r { result.push(n); } }
        Ok(result)
    }).await?;

    let mut topic_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut topic_latest: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for r in &topic_rows {
        topic_counts.insert(r.topic.clone(), r.count as usize);
        topic_latest.insert(r.topic.clone(), r.latest.clone());
    }

    for (topic, count) in &topic_counts {
        if *count <= 2 && !topic.is_empty() {
            gaps.push(KnowledgeGap {
                topic: topic.clone(),
                reason: format!("Topic '{}' only has {} node(s) - shallow coverage", topic, count),
                priority: 0.4,
                domain: "technology".to_string(),
            });
        }
    }

    // 4. Temporal gaps: topics that were once active but have gone stale (30+ days)
    let now = chrono::Utc::now();
    for (topic, latest) in &topic_latest {
        let count = topic_counts.get(topic).copied().unwrap_or(0);
        if count < 3 { continue; } // Only flag topics that had real coverage
        if let Ok(last_date) = chrono::DateTime::parse_from_rfc3339(latest) {
            let days_stale = (now - last_date.with_timezone(&chrono::Utc)).num_days();
            if days_stale > 30 {
                let base_priority = 0.5;
                let staleness_boost = (days_stale as f64 / 30.0).min(1.0) * 0.3;
                gaps.push(KnowledgeGap {
                    topic: topic.clone(),
                    reason: format!("Topic '{}' has {} nodes but no new content in {} days - going stale", topic, count, days_stale),
                    priority: base_priority + staleness_boost,
                    domain: "technology".to_string(),
                });
            }
        }
    }

    // Sort by priority descending
    gaps.sort_by(|a, b| b.priority.partial_cmp(&a.priority).unwrap_or(std::cmp::Ordering::Equal));
    gaps.truncate(20);

    Ok(gaps)
}
