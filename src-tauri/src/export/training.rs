//! Training data generation for fine-tuning — generates instruction/response pairs.
//!
//! Many of the row structs here have `title` fields used only for log
//! messages, not for the JSONL output. Suppress dead-code warnings module-wide.
#![allow(dead_code)]

use crate::db::BrainDb;
use crate::error::BrainError;
use std::io::Write;
use std::path::Path;

/// Generate training dataset in Alpaca instruction format.
/// Uses high-quality nodes and synthesis nodes as source material.
pub async fn generate_training_dataset(
    db: &BrainDb,
    format: &str,
    path: &str,
) -> Result<u64, BrainError> {
    #[derive(Debug)]
    struct TrainNode {
        title: String,
        content: String,
        summary: String,
        domain: String,
        topic: String,
        quality_score: f64,
        tags: Vec<String>,
        node_type: String,
    }

    if let Some(parent) = Path::new(path).parent() {
        std::fs::create_dir_all(parent).map_err(BrainError::Io)?;
    }
    let file = std::fs::File::create(path).map_err(BrainError::Io)?;
    let mut w = std::io::BufWriter::new(file);
    let mut count = 0u64;

    // Load high-quality non-file nodes
    let nodes: Vec<TrainNode> = db.with_conn(|conn| -> Result<Vec<TrainNode>, BrainError> {
        let mut stmt = conn.prepare(
            "SELECT title, content, summary, domain, topic, quality_score, tags, node_type \
             FROM nodes WHERE quality_score > 0.5 AND source_type != 'file' \
             ORDER BY quality_score DESC LIMIT 3000"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            let tags_json: String = row.get(6)?;
            Ok(TrainNode {
                title: row.get(0)?,
                content: row.get(1)?,
                summary: row.get(2)?,
                domain: row.get(3)?,
                topic: row.get(4)?,
                quality_score: row.get(5)?,
                tags: serde_json::from_str(&tags_json).unwrap_or_default(),
                node_type: row.get(7)?,
            })
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(n) = r { result.push(n); } }
        Ok(result)
    }).await?;

    // Also load synthesis nodes (highest value for training)
    let synthesis_nodes: Vec<TrainNode> = db.with_conn(|conn| -> Result<Vec<TrainNode>, BrainError> {
        let mut stmt = conn.prepare(
            "SELECT title, content, summary, domain, topic, quality_score, tags, node_type \
             FROM nodes WHERE node_type = 'synthesis' \
             ORDER BY quality_score DESC LIMIT 500"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            let tags_json: String = row.get(6)?;
            Ok(TrainNode {
                title: row.get(0)?,
                content: row.get(1)?,
                summary: row.get(2)?,
                domain: row.get(3)?,
                topic: row.get(4)?,
                quality_score: row.get(5)?,
                tags: serde_json::from_str(&tags_json).unwrap_or_default(),
                node_type: row.get(7)?,
            })
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(n) = r { result.push(n); } }
        Ok(result)
    }).await?;

    let all_nodes: Vec<&TrainNode> = nodes.iter().chain(synthesis_nodes.iter()).collect();

    for node in &all_nodes {
        let content_preview = if node.content.len() > 2000 {
            &node.content[..2000]
        } else {
            &node.content
        };

        match format {
            "alpaca" => {
                // Alpaca format: instruction -> output
                let instruction = format!("Explain {} in the context of {}.", node.title, node.domain);
                let entry = serde_json::json!({
                    "instruction": instruction,
                    "input": "",
                    "output": content_preview,
                    "quality_weight": node.quality_score,
                });
                serde_json::to_writer(&mut w, &entry).map_err(BrainError::Serialization)?;
                writeln!(w).map_err(BrainError::Io)?;
                count += 1;

                // If node has a good summary, create a summary training pair
                if !node.summary.ends_with("...") && node.summary.len() > 50 {
                    let entry = serde_json::json!({
                        "instruction": format!("Summarize the key points about {}.", node.title),
                        "input": "",
                        "output": node.summary,
                        "quality_weight": node.quality_score,
                    });
                    serde_json::to_writer(&mut w, &entry).map_err(BrainError::Serialization)?;
                    writeln!(w).map_err(BrainError::Io)?;
                    count += 1;
                }
            }
            "sharegpt" => {
                let entry = serde_json::json!({
                    "conversations": [
                        { "from": "human", "value": format!("What do you know about {}?", node.title) },
                        { "from": "gpt", "value": content_preview }
                    ],
                    "quality_weight": node.quality_score,
                });
                serde_json::to_writer(&mut w, &entry).map_err(BrainError::Serialization)?;
                writeln!(w).map_err(BrainError::Io)?;
                count += 1;
            }
            _ => {
                // Default JSONL
                let entry = serde_json::json!({
                    "type": "knowledge",
                    "title": node.title,
                    "content": content_preview,
                    "summary": node.summary,
                    "domain": node.domain,
                    "topic": node.topic,
                    "quality": (node.quality_score * 100.0).round() / 100.0,
                    "tags": node.tags,
                    "node_type": node.node_type,
                });
                serde_json::to_writer(&mut w, &entry).map_err(BrainError::Serialization)?;
                writeln!(w).map_err(BrainError::Io)?;
                count += 1;
            }
        }
    }

    w.flush().map_err(BrainError::Io)?;
    log::info!("Training dataset generated: {} entries in {} format at {}", count, format, path);
    Ok(count)
}

// =========================================================================
// Phase 2.3 — Personalized fine-tuning export
// =========================================================================
//
// Generates Q&A pairs that make a fine-tuned local model actually
// understand the user specifically. The general-purpose `generate_training_dataset`
// above produces "what do you know about X" pairs from any node — useful
// but generic. This function targets the parts of the brain that capture
// the user's unique behavior:
//
//  1. **Decision nodes** → "Why did we choose X for Y?" pairs
//  2. **user_cognition rules** → "How does the user prefer to do X?" pairs
//  3. **insight thinking nodes** → "What's the brain's insight on X?" pairs
//  4. **summary_cluster nodes** → "Summarize what we know about topic X" pairs
//
// All output goes into `~/.neurovault/export/training-personal.jsonl`
// in OpenAI fine-tune format (messages array). Run as a scheduled
// autonomy task every 24 hours so the dataset stays fresh as the brain
// grows.

/// Export personalized fine-tune-ready Q&A pairs to a JSONL file in
/// OpenAI chat-format ({messages: [{role, content}]}). Returns the
/// number of pairs written.
pub async fn export_personal_training(
    db: &BrainDb,
    path: &str,
) -> Result<u64, BrainError> {
    use std::io::Write;

    if let Some(parent) = Path::new(path).parent() {
        std::fs::create_dir_all(parent).map_err(BrainError::Io)?;
    }
    let file = std::fs::File::create(path).map_err(BrainError::Io)?;
    let mut w = std::io::BufWriter::new(file);
    let mut count = 0u64;

    let system_message = "You are NeuroVault, a personal AI assistant fine-tuned on the user's \
        knowledge graph. Answer questions using the established patterns, decisions, and \
        preferences extracted from his work.";

    // ---- 1. Decision nodes → why-pairs ----
    struct DecisionRow { title: String, content: String, topic: String }
    let decisions: Vec<DecisionRow> = db.with_conn(|conn| -> Result<Vec<DecisionRow>, BrainError> {
        let mut stmt = conn.prepare(
            "SELECT title, content, topic FROM nodes WHERE node_type = 'decision' LIMIT 500"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok(DecisionRow {
                title: row.get(0)?,
                content: row.get(1)?,
                topic: row.get(2)?,
            })
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(n) = r { result.push(n); } }
        Ok(result)
    }).await?;

    for d in &decisions {
        let q = format!("What did we decide about {}?", d.topic);
        let entry = serde_json::json!({
            "messages": [
                {"role": "system", "content": system_message},
                {"role": "user", "content": q},
                {"role": "assistant", "content": d.content},
            ],
            "source": "decision",
        });
        serde_json::to_writer(&mut w, &entry).map_err(BrainError::Serialization)?;
        writeln!(w).map_err(BrainError::Io)?;
        count += 1;
    }

    // ---- 2. user_cognition rules → preference pairs ----
    struct CogRow { pattern_type: String, extracted_rule: String, confidence: f32, times_confirmed: u32 }
    let cogs: Vec<CogRow> = db.with_conn(|conn| -> Result<Vec<CogRow>, BrainError> {
        let mut stmt = conn.prepare(
            "SELECT pattern_type, extracted_rule, confidence, times_confirmed \
             FROM user_cognition WHERE confidence > 0.6 LIMIT 500"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok(CogRow {
                pattern_type: row.get(0)?,
                extracted_rule: row.get(1)?,
                confidence: row.get(2)?,
                times_confirmed: row.get(3)?,
            })
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(n) = r { result.push(n); } }
        Ok(result)
    }).await?;

    for c in &cogs {
        // Skip rules with low evidence
        if c.times_confirmed < 2 { continue; }
        let q = match c.pattern_type.as_str() {
            "coding_style" => "How does the user prefer to write code?",
            "naming" => "What naming conventions does the user use?",
            "debug_flow" => "How does the user typically debug?",
            "planning" => "How does the user approach planning?",
            "refactoring" => "How does the user refactor code?",
            "tooling" => "What tools does the user prefer?",
            "communication" => "How does the user communicate technical ideas?",
            "decision_making" => "How does the user make technical decisions?",
            "testing" => "How does the user write tests?",
            "error_handling" => "How does the user handle errors?",
            _ => "What's a known preference of the user's?",
        };
        let entry = serde_json::json!({
            "messages": [
                {"role": "system", "content": system_message},
                {"role": "user", "content": q},
                {"role": "assistant", "content": c.extracted_rule},
            ],
            "source": "user_cognition",
            "confidence": c.confidence,
        });
        serde_json::to_writer(&mut w, &entry).map_err(BrainError::Serialization)?;
        writeln!(w).map_err(BrainError::Io)?;
        count += 1;
    }

    // ---- 3. Insight thinking nodes → "what's the insight" pairs ----
    struct InsightRow { _title: String, content: String, topic: String, confidence: Option<f64> }
    let insights: Vec<InsightRow> = db.with_conn(|conn| -> Result<Vec<InsightRow>, BrainError> {
        let mut stmt = conn.prepare(
            "SELECT title, content, topic, confidence FROM nodes \
             WHERE node_type IN ('insight', 'hypothesis', 'strategy') AND synthesized_by_brain = 1 \
             LIMIT 500"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok(InsightRow {
                _title: row.get(0)?,
                content: row.get(1)?,
                topic: row.get(2)?,
                confidence: row.get(3)?,
            })
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(n) = r { result.push(n); } }
        Ok(result)
    }).await?;

    for i in &insights {
        let q = format!("What insight has the brain captured about {}?", i.topic);
        let entry = serde_json::json!({
            "messages": [
                {"role": "system", "content": system_message},
                {"role": "user", "content": q},
                {"role": "assistant", "content": i.content},
            ],
            "source": "thinking_node",
            "confidence": i.confidence.unwrap_or(0.5),
        });
        serde_json::to_writer(&mut w, &entry).map_err(BrainError::Serialization)?;
        writeln!(w).map_err(BrainError::Io)?;
        count += 1;
    }

    // ---- 4. summary_cluster nodes → "summarise topic" pairs ----
    struct ClusterRow { _title: String, content: String, topic: String }
    let clusters: Vec<ClusterRow> = db.with_conn(|conn| -> Result<Vec<ClusterRow>, BrainError> {
        let mut stmt = conn.prepare(
            "SELECT title, content, topic FROM nodes WHERE node_type = 'summary_cluster' LIMIT 200"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok(ClusterRow {
                _title: row.get(0)?,
                content: row.get(1)?,
                topic: row.get(2)?,
            })
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(n) = r { result.push(n); } }
        Ok(result)
    }).await?;

    for c in &clusters {
        let q = format!("Give me a dense summary of everything the brain knows about {}.", c.topic);
        let entry = serde_json::json!({
            "messages": [
                {"role": "system", "content": system_message},
                {"role": "user", "content": q},
                {"role": "assistant", "content": c.content},
            ],
            "source": "summary_cluster",
        });
        serde_json::to_writer(&mut w, &entry).map_err(BrainError::Serialization)?;
        writeln!(w).map_err(BrainError::Io)?;
        count += 1;
    }

    w.flush().map_err(BrainError::Io)?;
    log::info!(
        "Personal training export: {} pairs written to {} ({} decisions, {} preferences, {} insights, {} clusters)",
        count, path, decisions.len(), cogs.len(), insights.len(), clusters.len()
    );
    Ok(count)
}
