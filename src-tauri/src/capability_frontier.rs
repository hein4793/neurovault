//! Capability Frontier — Phase Omega Part IV
//!
//! Tracks the brain's knowledge capabilities: what domains and topics it
//! covers, how proficient it is, where gaps exist, and how proficiency
//! changes over time.

use crate::db::BrainDb;
use crate::error::BrainError;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// =========================================================================
// Types
// =========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capability {
    pub id: String,
    pub name: String,
    pub proficiency: f32,
    pub evidence_count: u32,
    pub last_tested: String,
    pub status: String, // "mastered", "competent", "learning", "gap"
    pub improvement_plan: Option<String>,
}

// =========================================================================
// inventory_capabilities — scan domains, topics, expertise
// =========================================================================

pub async fn inventory_capabilities(db: &Arc<BrainDb>) -> Result<Vec<Capability>, BrainError> {
    // 1. Aggregate knowledge coverage by domain+topic
    let domain_counts: Vec<(String, u32, f64)> = db
        .with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT domain, COUNT(*) as cnt, AVG(quality_score) as avg_q
                     FROM nodes
                     WHERE domain != '' AND domain != 'general'
                     GROUP BY domain
                     ORDER BY cnt DESC
                     LIMIT 50",
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, u32>(1)?,
                        row.get::<_, f64>(2).unwrap_or(0.5),
                    ))
                })
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let mut result = Vec::new();
            for r in rows {
                if let Ok(v) = r {
                    result.push(v);
                }
            }
            Ok(result)
        })
        .await?;

    // 2. Also pull cognitive fingerprint expertise if available
    let expertise: std::collections::HashMap<String, f64> = db
        .with_conn(|conn| {
            let result = conn.query_row(
                "SELECT expertise FROM cognitive_fingerprint LIMIT 1",
                [],
                |row| row.get::<_, String>(0),
            );
            match result {
                Ok(json_str) => {
                    let map: std::collections::HashMap<String, f64> =
                        serde_json::from_str(&json_str).unwrap_or_default();
                    Ok(map)
                }
                Err(_) => Ok(std::collections::HashMap::new()),
            }
        })
        .await?;

    let now = chrono::Utc::now().to_rfc3339();
    let max_count = domain_counts
        .iter()
        .map(|(_, c, _)| *c)
        .max()
        .unwrap_or(1)
        .max(1);

    let mut capabilities = Vec::new();

    for (domain, count, avg_quality) in &domain_counts {
        // Proficiency combines: coverage breadth, quality, and fingerprint expertise
        let coverage_score = (*count as f32 / max_count as f32).min(1.0);
        let quality_score = *avg_quality as f32;
        let expertise_score = expertise
            .get(domain)
            .map(|v| *v as f32)
            .unwrap_or(coverage_score * 0.5);

        let proficiency = (coverage_score * 0.3 + quality_score * 0.3 + expertise_score * 0.4)
            .clamp(0.0, 1.0);

        let status = if proficiency >= 0.8 {
            "mastered"
        } else if proficiency >= 0.5 {
            "competent"
        } else if *count >= 3 {
            "learning"
        } else {
            "gap"
        };

        let cap_id = format!("cap:{}", domain.replace(' ', "_").to_lowercase());

        // Upsert into capabilities table
        let cid = cap_id.clone();
        let cname = domain.clone();
        let prof = proficiency;
        let ec = *count;
        let st = status.to_string();
        let ts = now.clone();
        let _ = db
            .with_conn(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO capabilities
                     (id, name, proficiency, evidence_count, last_tested,
                      status, improvement_plan, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7)",
                    params![cid, cname, prof, ec, ts, st, ts],
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
                Ok(())
            })
            .await;

        capabilities.push(Capability {
            id: cap_id,
            name: domain.clone(),
            proficiency,
            evidence_count: *count,
            last_tested: now.clone(),
            status: status.to_string(),
            improvement_plan: None,
        });
    }

    Ok(capabilities)
}

// =========================================================================
// test_capability — generate a test question for a domain, self-evaluate
// =========================================================================

pub async fn test_capability(db: &Arc<BrainDb>, name: &str) -> Result<Capability, BrainError> {
    // Fetch some nodes from this domain for context
    let cap_name = name.to_string();
    let sample_nodes: Vec<(String, String)> = db
        .with_conn(move |conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT title, summary FROM nodes
                     WHERE domain = ?1
                     ORDER BY quality_score DESC
                     LIMIT 10",
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt
                .query_map(params![cap_name], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let mut result = Vec::new();
            for r in rows {
                if let Ok(v) = r {
                    result.push(v);
                }
            }
            Ok(result)
        })
        .await?;

    if sample_nodes.is_empty() {
        return Ok(Capability {
            id: format!("cap:{}", name.replace(' ', "_").to_lowercase()),
            name: name.to_string(),
            proficiency: 0.0,
            evidence_count: 0,
            last_tested: chrono::Utc::now().to_rfc3339(),
            status: "gap".to_string(),
            improvement_plan: Some(format!("No knowledge found for domain '{}'. Needs research.", name)),
        });
    }

    let mut context = String::new();
    for (title, summary) in &sample_nodes {
        context.push_str(&format!("- {}: {}\n", title, summary));
    }

    // Generate a test question
    let llm = crate::commands::ai::get_llm_client_fast(db);
    let gen_prompt = format!(
        "Based on these knowledge nodes about '{}', generate ONE specific technical \
         question that tests deep understanding. Output only the question, no preamble.\n\n{}",
        name, context
    );
    let question = llm.generate(&gen_prompt, 200).await?;

    // Now answer it using the brain's knowledge
    let answer_prompt = format!(
        "Answer this question using ONLY the knowledge provided below. \
         Rate your confidence 0-100 at the end as CONFIDENCE:XX\n\n\
         Question: {}\n\nKnowledge:\n{}",
        question, context
    );
    let answer = llm.generate(&answer_prompt, 400).await?;

    // Parse confidence from the answer
    let confidence: f32 = answer
        .lines()
        .rev()
        .find_map(|line| {
            if let Some(pos) = line.to_uppercase().find("CONFIDENCE:") {
                let num_str = line[pos + 11..].trim().trim_matches(|c: char| !c.is_numeric());
                num_str.parse::<f32>().ok().map(|v| v / 100.0)
            } else {
                None
            }
        })
        .unwrap_or(0.5);

    let proficiency = confidence.clamp(0.0, 1.0);
    let status = if proficiency >= 0.8 {
        "mastered"
    } else if proficiency >= 0.5 {
        "competent"
    } else if sample_nodes.len() >= 3 {
        "learning"
    } else {
        "gap"
    };

    let now = chrono::Utc::now().to_rfc3339();
    let cap_id = format!("cap:{}", name.replace(' ', "_").to_lowercase());

    // Update in DB
    let cid = cap_id.clone();
    let cname = name.to_string();
    let prof = proficiency;
    let ec = sample_nodes.len() as u32;
    let st = status.to_string();
    let ts = now.clone();
    let _ = db
        .with_conn(move |conn| {
            conn.execute(
                "INSERT OR REPLACE INTO capabilities
                 (id, name, proficiency, evidence_count, last_tested,
                  status, improvement_plan, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7)",
                params![cid, cname, prof, ec, ts, st, ts],
            )
            .map_err(|e| BrainError::Database(e.to_string()))?;
            Ok(())
        })
        .await;

    Ok(Capability {
        id: cap_id,
        name: name.to_string(),
        proficiency,
        evidence_count: sample_nodes.len() as u32,
        last_tested: now,
        status: status.to_string(),
        improvement_plan: None,
    })
}

// =========================================================================
// identify_gaps — find capabilities the user needs but brain lacks
// =========================================================================

pub async fn identify_gaps(db: &Arc<BrainDb>) -> Result<Vec<Capability>, BrainError> {
    // 1. Get all capabilities
    let caps = inventory_capabilities(db).await?;

    // 2. Check user_cognition and decisions for mentioned but uncovered domains
    let mentioned_domains: Vec<String> = db
        .with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT DISTINCT domain FROM nodes
                     WHERE domain != '' AND domain != 'general'
                     GROUP BY domain
                     HAVING COUNT(*) < 5",
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let mut result = Vec::new();
            for r in rows {
                if let Ok(v) = r {
                    result.push(v);
                }
            }
            Ok(result)
        })
        .await?;

    // Filter to only gaps and learning-stage capabilities
    let mut gaps: Vec<Capability> = caps
        .into_iter()
        .filter(|c| c.status == "gap" || c.status == "learning")
        .collect();

    // Add mentioned but poorly covered domains
    let existing_names: std::collections::HashSet<String> =
        gaps.iter().map(|c| c.name.clone()).collect();
    let now = chrono::Utc::now().to_rfc3339();

    for domain in mentioned_domains {
        if !existing_names.contains(&domain) {
            gaps.push(Capability {
                id: format!("cap:{}", domain.replace(' ', "_").to_lowercase()),
                name: domain,
                proficiency: 0.1,
                evidence_count: 0,
                last_tested: now.clone(),
                status: "gap".to_string(),
                improvement_plan: Some("Needs dedicated research".to_string()),
            });
        }
    }

    // Sort by proficiency ascending (worst gaps first)
    gaps.sort_by(|a, b| {
        a.proficiency
            .partial_cmp(&b.proficiency)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(gaps)
}

// =========================================================================
// track_frontier — update proficiency levels from recent performance
// =========================================================================

pub async fn track_frontier(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    let caps = inventory_capabilities(db).await?;
    let gaps = identify_gaps(db).await?;

    let mastered = caps.iter().filter(|c| c.status == "mastered").count();
    let competent = caps.iter().filter(|c| c.status == "competent").count();
    let learning = caps.iter().filter(|c| c.status == "learning").count();
    let gap_count = gaps.len();

    Ok(format!(
        "Capability frontier: {} mastered, {} competent, {} learning, {} gaps identified",
        mastered, competent, learning, gap_count
    ))
}
