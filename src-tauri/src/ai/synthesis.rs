//! Knowledge Synthesis Engine — creates insight nodes from topic clusters.

use crate::db::BrainDb;
use crate::db::models::{CreateNodeInput, CreateEdgeInput};
use crate::error::BrainError;
use rusqlite::params;

/// Run knowledge synthesis: find topic clusters and create insight nodes.
pub async fn run_synthesis(db: &BrainDb) -> Result<String, BrainError> {
    let llm = crate::commands::ai::get_llm_client(db);

    // Find topics with 5+ nodes that don't already have a synthesis node
    let clusters: Vec<(String, u64)> = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT topic, COUNT(*) as cnt FROM nodes \
             WHERE topic != '' AND node_type != 'synthesis' \
             GROUP BY topic HAVING cnt >= 5 LIMIT 10"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?))
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(c) = r { result.push(c); } }
        Ok(result)
    }).await?;

    // Check which topics already have synthesis nodes
    let synthesized: std::collections::HashSet<String> = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT topic FROM nodes WHERE node_type = 'synthesis'"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| BrainError::Database(e.to_string()))?;
        let mut set = std::collections::HashSet::new();
        for r in rows { if let Ok(t) = r { set.insert(t); } }
        Ok(set)
    }).await?;

    let mut insights_created = 0u32;

    for (topic, _count) in clusters.iter().take(5) {
        if synthesized.contains(topic) { continue; }

        // Load summaries of nodes in this topic
        let topic_clone = topic.clone();
        let nodes: Vec<(String, String, String)> = db.with_conn(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, title, summary FROM nodes \
                 WHERE topic = ?1 AND node_type != 'synthesis' \
                 ORDER BY quality_score DESC LIMIT 15"
            ).map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt.query_map(params![topic_clone], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
            }).map_err(|e| BrainError::Database(e.to_string()))?;
            let mut result = Vec::new();
            for r in rows { if let Ok(n) = r { result.push(n); } }
            Ok(result)
        }).await?;

        if nodes.len() < 3 { continue; }

        let summaries_text: String = nodes.iter()
            .map(|(_, title, summary)| format!("- {}: {}", title, summary))
            .collect::<Vec<_>>()
            .join("\n");

        let prompt = format!(
            "You are synthesizing {} knowledge nodes about '{}' into a deep insight.\n\n\
             STEP 1: Identify the 3 core concepts across these nodes.\n\
             STEP 2: Map how they connect to each other.\n\
             STEP 3: What patterns or principles emerge?\n\
             STEP 4: What practical implications does this have?\n\n\
             NODE SUMMARIES:\n{}\n\n\
             Write a comprehensive synthesis (3-5 paragraphs). Be specific and reference concepts from the nodes.\n\
             Start with the most important insight, not with \"This synthesis covers...\"",
            nodes.len(), topic, summaries_text
        );

        match llm.generate(&prompt, 1500).await {
            Ok(synthesis_text) => {
                let title = format!("Synthesis: {}", topic);
                match db.create_node(CreateNodeInput {
                    title: title.clone(),
                    content: synthesis_text,
                    domain: "reference".to_string(),
                    topic: topic.clone(),
                    tags: vec!["synthesis".to_string(), "insight".to_string(), topic.clone()],
                    node_type: "synthesis".to_string(),
                    source_type: "synthesis".to_string(),
                    source_url: None,
                }).await {
                    Ok(insight_node) => {
                        for (source_id, _, _) in &nodes {
                            let _ = db.create_edge(CreateEdgeInput {
                                source_id: source_id.clone(),
                                target_id: insight_node.id.clone(),
                                relation_type: "synthesized_into".to_string(),
                                evidence: format!("Synthesized from {} nodes about '{}'", nodes.len(), topic),
                            }).await;
                        }
                        insights_created += 1;
                        log::info!("Synthesis: created insight for '{}'", topic);
                    }
                    Err(e) => log::warn!("Failed to create synthesis node for '{}': {}", topic, e),
                }
            }
            Err(e) => log::warn!("LLM synthesis failed for '{}': {}", topic, e),
        }

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }

    Ok(format!("Created {} synthesis insights", insights_created))
}
