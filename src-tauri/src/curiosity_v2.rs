//! Phase Omega Part IX — Advanced Curiosity Engine v2
//!
//! Computes information gain for potential research topics, ranks them by
//! strategic value, and adds 10% serendipitous random exploration to avoid
//! local optima. Tracks learning velocity per domain to measure how fast
//! the brain absorbs knowledge.

use crate::db::BrainDb;
use crate::error::BrainError;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

// =========================================================================
// DATA STRUCTURES
// =========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CuriosityTarget {
    pub topic: String,
    pub expected_information_gain: f32,
    pub novelty: f32,
    pub connectivity_potential: f32,
    pub user_relevance: f32,
    pub gap_fill_value: f32,
    pub is_serendipity: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningVelocity {
    pub domain: String,
    pub nodes_per_day: f32,
    pub quality_trend: f32,
    pub last_computed: String,
}

// =========================================================================
// COMPUTE INFORMATION GAIN
// =========================================================================

/// For each potential research topic, calculate expected information gain
/// based on: how many related nodes exist (novelty inversely proportional),
/// how many edges could form (connectivity potential), alignment with user
/// interests, and gap-fill value from sparse domains.
pub async fn compute_information_gain(db: &Arc<BrainDb>) -> Result<Vec<CuriosityTarget>, BrainError> {
    // --- Gather domain stats for gap detection ---
    let domain_stats: HashMap<String, (u64, f64)> = db
        .with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT domain, COUNT(*) as cnt, AVG(quality_score) as avg_q \
                     FROM nodes WHERE domain != '' \
                     GROUP BY domain",
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, u64>(1)?,
                        row.get::<_, f64>(2)?,
                    ))
                })
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let mut map = HashMap::new();
            for row in rows {
                if let Ok((domain, count, avg_q)) = row {
                    map.insert(domain, (count, avg_q));
                }
            }
            Ok(map)
        })
        .await?;

    let total_nodes: u64 = domain_stats.values().map(|(c, _)| *c).sum();
    let avg_domain_size = if domain_stats.is_empty() {
        1.0
    } else {
        total_nodes as f64 / domain_stats.len() as f64
    };

    // --- User interest domains from cognition rules ---
    let user_domains: Vec<String> = db
        .with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT DISTINCT n.domain \
                     FROM user_cognition uc \
                     JOIN nodes n ON n.id IN ( \
                         SELECT value FROM json_each(uc.linked_to_nodes) \
                     ) \
                     WHERE uc.confidence > 0.4 AND n.domain != '' \
                     LIMIT 20",
                )
                .map_err(|e| BrainError::Database(e.to_string()));
            match stmt {
                Ok(ref mut s) => {
                    let rows = s
                        .query_map([], |row| row.get::<_, String>(0))
                        .map_err(|e| BrainError::Database(e.to_string()))?;
                    let mut results = Vec::new();
                    for row in rows {
                        if let Ok(r) = row {
                            results.push(r);
                        }
                    }
                    Ok(results)
                }
                Err(_) => Ok(Vec::new()),
            }
        })
        .await?;

    // --- Candidate topics from various sources ---
    let mut candidates: Vec<CuriosityTarget> = Vec::new();

    // Source 1: Sparse domains (high gap-fill value)
    for (domain, (count, _avg_q)) in &domain_stats {
        let relative_size = *count as f64 / avg_domain_size;
        if relative_size < 0.5 && *count >= 2 {
            // Under-represented domain
            let novelty = 1.0 - (relative_size as f32).min(1.0);
            let gap_fill = (1.0 - relative_size as f32).max(0.0);
            let user_rel = if user_domains.contains(domain) {
                1.0f32
            } else {
                0.2
            };
            let connectivity = (relative_size as f32 * 0.5).min(1.0); // more nodes = more links possible
            let info_gain = novelty * 0.25 + connectivity * 0.20 + user_rel * 0.25 + gap_fill * 0.30;

            candidates.push(CuriosityTarget {
                topic: format!("{} (deep dive)", domain),
                expected_information_gain: info_gain,
                novelty,
                connectivity_potential: connectivity,
                user_relevance: user_rel,
                gap_fill_value: gap_fill,
                is_serendipity: false,
            });
        }
    }

    // Source 2: Low-quality domains that could benefit from research
    for (domain, (count, avg_q)) in &domain_stats {
        if *avg_q < 0.5 && *count >= 3 {
            let novelty = 0.3; // not novel but needs improvement
            let gap_fill = (1.0 - *avg_q as f32).max(0.0);
            let user_rel = if user_domains.contains(domain) {
                1.0f32
            } else {
                0.3
            };
            let connectivity = (*count as f32 / total_nodes.max(1) as f32).min(1.0);
            let info_gain =
                novelty * 0.20 + connectivity * 0.20 + user_rel * 0.25 + gap_fill * 0.35;

            candidates.push(CuriosityTarget {
                topic: format!("{} (quality improvement)", domain),
                expected_information_gain: info_gain,
                novelty,
                connectivity_potential: connectivity,
                user_relevance: user_rel,
                gap_fill_value: gap_fill,
                is_serendipity: false,
            });
        }
    }

    // Source 3: Topics from recent nodes that could branch into new areas
    let branching_topics: Vec<(String, String)> = db
        .with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT topic, domain FROM nodes \
                     WHERE topic != '' AND created_at >= DATETIME('now', '-14 days') \
                     GROUP BY topic \
                     HAVING COUNT(*) BETWEEN 1 AND 3 \
                     ORDER BY MAX(quality_score) DESC \
                     LIMIT 20",
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let mut results = Vec::new();
            for row in rows {
                if let Ok(r) = row {
                    results.push(r);
                }
            }
            Ok(results)
        })
        .await?;

    for (topic, domain) in &branching_topics {
        let domain_count = domain_stats
            .get(domain)
            .map(|(c, _)| *c)
            .unwrap_or(0);
        let novelty = 0.7; // partially explored
        let connectivity = (domain_count as f32 / total_nodes.max(1) as f32 * 5.0).min(1.0);
        let user_rel = if user_domains.contains(domain) {
            0.8f32
        } else {
            0.3
        };
        let gap_fill = 0.4;
        let info_gain = novelty * 0.30 + connectivity * 0.20 + user_rel * 0.25 + gap_fill * 0.25;

        candidates.push(CuriosityTarget {
            topic: topic.clone(),
            expected_information_gain: info_gain,
            novelty,
            connectivity_potential: connectivity,
            user_relevance: user_rel,
            gap_fill_value: gap_fill,
            is_serendipity: false,
        });
    }

    // Source 4: Serendipitous random exploration (10%)
    let random_topics: Vec<String> = db
        .with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT DISTINCT topic FROM nodes \
                     WHERE topic != '' \
                     ORDER BY RANDOM() \
                     LIMIT 5",
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let mut results = Vec::new();
            for row in rows {
                if let Ok(r) = row {
                    results.push(r);
                }
            }
            Ok(results)
        })
        .await?;

    for topic in &random_topics {
        candidates.push(CuriosityTarget {
            topic: format!("{} (serendipity)", topic),
            expected_information_gain: 0.5, // moderate baseline
            novelty: 0.9,
            connectivity_potential: 0.4,
            user_relevance: 0.3,
            gap_fill_value: 0.3,
            is_serendipity: true,
        });
    }

    // Sort by information gain
    candidates.sort_by(|a, b| {
        b.expected_information_gain
            .partial_cmp(&a.expected_information_gain)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Deduplicate by topic
    let mut seen = std::collections::HashSet::new();
    candidates.retain(|c| seen.insert(c.topic.clone()));

    Ok(candidates)
}

// =========================================================================
// GET CURIOSITY TARGETS
// =========================================================================

/// Return the best topics to research: 90% strategic + 10% serendipitous.
pub async fn get_curiosity_targets(
    db: &Arc<BrainDb>,
    limit: usize,
) -> Result<Vec<CuriosityTarget>, BrainError> {
    let all = compute_information_gain(db).await?;

    let strategic_count = (limit as f32 * 0.9).ceil() as usize;
    let serendipity_count = limit.saturating_sub(strategic_count);

    let mut result: Vec<CuriosityTarget> = Vec::new();

    // Strategic picks (non-serendipity, sorted by info gain)
    let strategic: Vec<&CuriosityTarget> = all.iter().filter(|c| !c.is_serendipity).collect();
    for c in strategic.iter().take(strategic_count) {
        result.push((*c).clone());
    }

    // Serendipitous picks
    let serendipitous: Vec<&CuriosityTarget> = all.iter().filter(|c| c.is_serendipity).collect();
    for c in serendipitous.iter().take(serendipity_count) {
        result.push((*c).clone());
    }

    // If we didn't get enough serendipitous, fill with more strategic
    if result.len() < limit {
        let remaining = limit - result.len();
        let already: std::collections::HashSet<String> =
            result.iter().map(|c| c.topic.clone()).collect();
        for c in all.iter() {
            if result.len() >= limit {
                break;
            }
            if !already.contains(&c.topic) {
                result.push(c.clone());
            }
        }
        let _ = remaining; // suppress unused warning
    }

    result.truncate(limit);
    Ok(result)
}

// =========================================================================
// TRACK LEARNING VELOCITY
// =========================================================================

/// Compute how fast the brain is learning in each domain: nodes per day
/// and quality trend (improving / declining).
pub async fn track_learning_velocity(db: &Arc<BrainDb>) -> Result<Vec<LearningVelocity>, BrainError> {
    let now = chrono::Utc::now().to_rfc3339();

    let velocities: Vec<LearningVelocity> = db
        .with_conn(move |conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT domain, \
                            COUNT(*) as total, \
                            JULIANDAY('now') - JULIANDAY(MIN(created_at)) as days_span, \
                            AVG(CASE WHEN created_at >= DATETIME('now', '-7 days') THEN quality_score END) as recent_q, \
                            AVG(CASE WHEN created_at < DATETIME('now', '-7 days') AND created_at >= DATETIME('now', '-30 days') THEN quality_score END) as older_q \
                     FROM nodes \
                     WHERE domain != '' AND created_at >= DATETIME('now', '-30 days') \
                     GROUP BY domain \
                     HAVING total >= 2",
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;

            let rows = stmt
                .query_map([], |row| {
                    let domain: String = row.get(0)?;
                    let total: f64 = row.get(1)?;
                    let days_span: f64 = row.get::<_, Option<f64>>(2)?.unwrap_or(1.0).max(1.0);
                    let recent_q: f64 = row.get::<_, Option<f64>>(3)?.unwrap_or(0.5);
                    let older_q: f64 = row.get::<_, Option<f64>>(4)?.unwrap_or(0.5);

                    let nodes_per_day = (total / days_span) as f32;
                    let quality_trend = (recent_q - older_q) as f32; // positive = improving

                    Ok(LearningVelocity {
                        domain,
                        nodes_per_day,
                        quality_trend,
                        last_computed: String::new(), // filled below
                    })
                })
                .map_err(|e| BrainError::Database(e.to_string()))?;

            let mut results = Vec::new();
            for row in rows {
                if let Ok(mut v) = row {
                    v.last_computed = now.clone();
                    results.push(v);
                }
            }
            Ok(results)
        })
        .await?;

    // Persist to learning_velocity table
    let vel_clone = velocities.clone();
    db.with_conn(move |conn| {
        for v in &vel_clone {
            conn.execute(
                "INSERT OR REPLACE INTO learning_velocity \
                 (domain, nodes_per_day, quality_trend, last_computed) \
                 VALUES (?1, ?2, ?3, ?4)",
                params![v.domain, v.nodes_per_day, v.quality_trend, v.last_computed],
            )
            .map_err(|e| BrainError::Database(e.to_string()))?;
        }
        Ok(())
    })
    .await?;

    log::info!(
        "Learning velocity tracked: {} domains",
        velocities.len()
    );
    Ok(velocities)
}
