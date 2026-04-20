//! Context Bundle — the 6-layer intelligence package injected into Claude Code.
//!
//! This is the bridge between NeuroVault and Claude Opus. Every few seconds
//! the sidekick builds a ContextBundle for the user's active Claude Code
//! session, compresses it to ~4000 tokens, and writes it as structured
//! markdown that Claude reads on every turn.
//!
//! ## Architecture
//!
//! ```text
//! Layer 1: Compiled Rules    (instant, no LLM — knowledge_rules table)
//! Layer 2: Knowledge Nodes   (HNSW + FTS5 → MMR-selected, compressed)
//! Layer 3: Work Patterns     (user_cognition — how the user works)
//! Layer 4: Decision Memory   (past decisions on this topic)
//! Layer 5: Warnings          (past mistakes, known pitfalls)
//! Layer 6: Predictions       (what the user will need next)
//! ```
//!
//! Each layer has a token budget. The compression engine ensures the total
//! bundle fits within ~4000 tokens while maximizing information density.

use crate::db::models::{SearchResult, UserCognition};
use crate::db::BrainDb;
use crate::error::BrainError;
use crate::knowledge_compiler::KnowledgeRule;
use rusqlite::params;
use std::collections::HashSet;

// =========================================================================
// Token budget constants
// =========================================================================

const BUDGET_RULES: usize = 300;
const BUDGET_KNOWLEDGE: usize = 2000;
const BUDGET_PATTERNS: usize = 500;
const BUDGET_DECISIONS: usize = 400;
const BUDGET_WARNINGS: usize = 300;
const BUDGET_PREDICTIONS: usize = 200;

const CHARS_PER_TOKEN: usize = 4;

// =========================================================================
// Bundle types
// =========================================================================

#[derive(Debug, Clone)]
pub struct ContextBundle {
    pub compiled_rules: Vec<MatchedRule>,
    pub knowledge_nodes: Vec<CompressedNode>,
    pub work_patterns: Vec<PatternEntry>,
    pub decisions: Vec<DecisionEntry>,
    pub warnings: Vec<WarningEntry>,
    pub predictions: Vec<PredictionEntry>,
    pub query: String,
    pub project: String,
    pub total_chars: usize,
    pub generation_ms: u64,
}

#[derive(Debug, Clone)]
pub struct MatchedRule {
    pub rule_type: String,
    pub condition: String,
    pub action: String,
    pub confidence: f32,
}

#[derive(Debug, Clone)]
pub struct CompressedNode {
    pub title: String,
    pub domain: String,
    pub summary: String,
    pub score: f32,
}

#[derive(Debug, Clone)]
pub struct PatternEntry {
    pub pattern_type: String,
    pub rule: String,
    pub confidence: f32,
    pub times_confirmed: u32,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DecisionEntry {
    pub title: String,
    pub summary: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct WarningEntry {
    pub title: String,
    pub warning: String,
    pub severity: String,
}

#[derive(Debug, Clone)]
pub struct PredictionEntry {
    pub prediction: String,
    pub confidence: f32,
    pub timeframe: String,
}

// =========================================================================
// Bundle builder
// =========================================================================

pub async fn build_context_bundle(
    db: &BrainDb,
    query: &str,
    project: &str,
) -> ContextBundle {
    let start = std::time::Instant::now();

    let (rules, knowledge, patterns, decisions, warnings, predictions) = tokio::join!(
        layer_compiled_rules(db, query),
        layer_knowledge_nodes(db, query),
        layer_work_patterns(db, query),
        layer_decision_memory(db, query),
        layer_warnings(db, query),
        layer_predictions(db, query),
    );

    let mut bundle = ContextBundle {
        compiled_rules: rules,
        knowledge_nodes: knowledge,
        work_patterns: patterns,
        decisions,
        warnings,
        predictions,
        query: query.to_string(),
        project: project.to_string(),
        total_chars: 0,
        generation_ms: start.elapsed().as_millis() as u64,
    };

    enforce_token_budget(&mut bundle);

    bundle.total_chars = count_bundle_chars(&bundle);
    bundle
}

// =========================================================================
// Layer 1: Compiled Rules (microseconds, no LLM)
// =========================================================================

async fn layer_compiled_rules(db: &BrainDb, query: &str) -> Vec<MatchedRule> {
    let ctx = query.to_lowercase();
    let ctx_words: HashSet<String> = ctx
        .split_whitespace()
        .filter(|w| w.len() > 2)
        .map(|s| s.trim_matches(|c: char| !c.is_alphanumeric()).to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let all_rules: Vec<KnowledgeRule> = db
        .with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT id, source_node_ids, rule_type, condition, action,
                            confidence, times_applied, times_correct, accuracy,
                            compiled_at, invalidated
                     FROM knowledge_rules
                     WHERE invalidated = 0
                     ORDER BY confidence DESC
                     LIMIT 100",
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt
                .query_map([], |row| {
                    Ok(KnowledgeRule {
                        id: row.get(0)?,
                        source_node_ids: serde_json::from_str(&row.get::<_, String>(1)?)
                            .unwrap_or_default(),
                        rule_type: row.get(2)?,
                        condition: row.get(3)?,
                        action: row.get(4)?,
                        confidence: row.get(5)?,
                        times_applied: row.get::<_, u32>(6)?,
                        times_correct: row.get::<_, u32>(7)?,
                        accuracy: row.get(8)?,
                        compiled_at: row.get(9)?,
                        invalidated: row.get::<_, i32>(10)? != 0,
                    })
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
        .await
        .unwrap_or_default();

    let mut matched: Vec<(MatchedRule, usize)> = Vec::new();
    for rule in all_rules {
        let cond_lower = rule.condition.to_lowercase();
        let cond_words: HashSet<String> = cond_lower
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|w| w.len() > 2)
            .map(|s| s.to_string())
            .collect();
        let overlap = ctx_words.intersection(&cond_words).count();
        if overlap >= 2 {
            matched.push((
                MatchedRule {
                    rule_type: rule.rule_type,
                    condition: rule.condition,
                    action: rule.action,
                    confidence: rule.confidence,
                },
                overlap,
            ));
        }
    }

    // "always" and "never" rules always apply regardless of overlap
    let always_never: Vec<MatchedRule> = db
        .with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT rule_type, condition, action, confidence
                     FROM knowledge_rules
                     WHERE invalidated = 0
                       AND rule_type IN ('always', 'never')
                     ORDER BY confidence DESC
                     LIMIT 10",
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt
                .query_map([], |row| {
                    Ok(MatchedRule {
                        rule_type: row.get(0)?,
                        condition: row.get(1)?,
                        action: row.get(2)?,
                        confidence: row.get(3)?,
                    })
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
        .await
        .unwrap_or_default();

    // Merge: always/never first, then context-matched sorted by overlap
    let mut seen_actions: HashSet<String> = HashSet::new();
    let mut final_rules: Vec<MatchedRule> = Vec::new();
    for r in always_never {
        if seen_actions.insert(r.action.clone()) {
            final_rules.push(r);
        }
    }
    matched.sort_by(|a, b| b.1.cmp(&a.1));
    for (r, _) in matched {
        if seen_actions.insert(r.action.clone()) {
            final_rules.push(r);
        }
    }
    final_rules.truncate(10);
    final_rules
}

// =========================================================================
// Layer 2: Knowledge Nodes (HNSW + FTS5 → MMR selection)
// =========================================================================

async fn layer_knowledge_nodes(db: &BrainDb, query: &str) -> Vec<CompressedNode> {
    let candidate_limit = 40;
    let final_limit = 20;

    // Get candidates from semantic search
    let mut candidates: Vec<SearchResult> = Vec::new();

    let client = crate::embeddings::OllamaClient::new(
        db.config.ollama_url.clone(),
        db.config.embedding_model.clone(),
    );
    if client.health_check().await {
        if let Ok(emb) = client.generate_embedding(query).await {
            if let Ok(results) = db.vector_search(emb, candidate_limit).await {
                candidates = results;
            }
        }
    }

    // Supplement with FTS5 if we got fewer than candidate_limit
    if candidates.len() < candidate_limit / 2 {
        if let Ok(fts) = db.search_nodes(query).await {
            let existing_ids: HashSet<String> =
                candidates.iter().map(|c| c.node.id.clone()).collect();
            for r in fts {
                if !existing_ids.contains(&r.node.id) {
                    candidates.push(r);
                }
                if candidates.len() >= candidate_limit {
                    break;
                }
            }
        }
    }

    if candidates.is_empty() {
        return Vec::new();
    }

    // MMR selection: greedily pick nodes that maximize relevance while
    // minimizing redundancy (measured by title similarity as a fast proxy).
    let selected = mmr_select(&candidates, final_limit);

    selected
        .into_iter()
        .map(|r| CompressedNode {
            title: r.node.title.clone(),
            domain: r.node.domain.clone(),
            summary: compress_content(&r.node.summary, &r.node.content, 200),
            score: r.score as f32,
        })
        .collect()
}

/// Maximal Marginal Relevance — select diverse, relevant results.
///
/// Uses title-word overlap as a fast diversity proxy (avoids needing
/// pairwise embedding distance, which would require loading all vectors).
fn mmr_select(candidates: &[SearchResult], k: usize) -> Vec<&SearchResult> {
    if candidates.len() <= k {
        return candidates.iter().collect();
    }

    let lambda: f32 = 0.7;
    let mut selected: Vec<usize> = Vec::with_capacity(k);
    let mut remaining: Vec<usize> = (0..candidates.len()).collect();

    let word_sets: Vec<HashSet<String>> = candidates
        .iter()
        .map(|c| {
            c.node
                .title
                .to_lowercase()
                .split_whitespace()
                .filter(|w| w.len() > 2)
                .map(|s| s.to_string())
                .collect()
        })
        .collect();

    if let Some(best_idx) = remaining
        .iter()
        .max_by(|a, b| {
            candidates[**a]
                .score
                .partial_cmp(&candidates[**b].score)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .copied()
    {
        selected.push(best_idx);
        remaining.retain(|&i| i != best_idx);
    }

    while selected.len() < k && !remaining.is_empty() {
        let mut best_mmr = f32::NEG_INFINITY;
        let mut best_idx = remaining[0];

        for &idx in &remaining {
            let relevance = candidates[idx].score as f32;

            let max_sim: f32 = selected
                .iter()
                .map(|&s| jaccard(&word_sets[idx], &word_sets[s]))
                .fold(0.0f32, f32::max);

            let mmr = lambda * relevance - (1.0 - lambda) * max_sim;
            if mmr > best_mmr {
                best_mmr = mmr;
                best_idx = idx;
            }
        }

        selected.push(best_idx);
        remaining.retain(|&i| i != best_idx);
    }

    selected.iter().map(|&i| &candidates[i]).collect()
}

fn jaccard(a: &HashSet<String>, b: &HashSet<String>) -> f32 {
    if a.is_empty() && b.is_empty() {
        return 0.0;
    }
    let inter = a.intersection(b).count() as f32;
    let union = a.union(b).count() as f32;
    inter / union
}

fn compress_content(summary: &str, content: &str, max_chars: usize) -> String {
    let source = if !summary.is_empty() && summary.len() > 20 {
        summary
    } else {
        content
    };
    let cleaned = source.trim().replace('\n', " ").replace("  ", " ");
    if cleaned.chars().count() <= max_chars {
        cleaned
    } else {
        let truncated: String = cleaned.chars().take(max_chars).collect();
        format!("{}...", truncated)
    }
}

// =========================================================================
// Layer 3: Work Patterns (user_cognition)
// =========================================================================

async fn layer_work_patterns(db: &BrainDb, query: &str) -> Vec<PatternEntry> {
    let q = query.to_lowercase();
    let query_words: HashSet<String> = q
        .split_whitespace()
        .filter(|w| w.len() > 2)
        .map(|s| s.to_string())
        .collect();

    let all: Vec<UserCognition> = db
        .with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT id, timestamp, trigger_node_ids, pattern_type,
                            extracted_rule, structured_rule, confidence,
                            times_confirmed, times_contradicted, embedding,
                            linked_to_nodes
                     FROM user_cognition
                     ORDER BY confidence DESC
                     LIMIT 80",
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt
                .query_map([], |row| {
                    Ok(UserCognition {
                        id: row.get(0)?,
                        timestamp: row.get(1)?,
                        trigger_node_ids: serde_json::from_str(&row.get::<_, String>(2)?)
                            .unwrap_or_default(),
                        pattern_type: row.get(3)?,
                        extracted_rule: row.get(4)?,
                        structured_rule: row.get(5)?,
                        confidence: row.get(6)?,
                        times_confirmed: row.get(7)?,
                        times_contradicted: row.get(8)?,
                        embedding: None,
                        linked_to_nodes: serde_json::from_str(&row.get::<_, String>(10)?)
                            .unwrap_or_default(),
                    })
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
        .await
        .unwrap_or_default();

    // Score each pattern by relevance to query + confidence
    let mut scored: Vec<(PatternEntry, f32)> = all
        .into_iter()
        .map(|p| {
            let rule_lower = p.extracted_rule.to_lowercase();
            let rule_words: HashSet<String> = rule_lower
                .split_whitespace()
                .filter(|w| w.len() > 2)
                .map(|s| s.to_string())
                .collect();
            let overlap = query_words.intersection(&rule_words).count() as f32;
            let conf_score = p.confidence * (p.times_confirmed as f32 + 1.0);
            let relevance = overlap * 0.4 + conf_score * 0.6;
            (
                PatternEntry {
                    pattern_type: p.pattern_type,
                    rule: p.extracted_rule,
                    confidence: p.confidence,
                    times_confirmed: p.times_confirmed as u32,
                },
                relevance,
            )
        })
        .collect();

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(8);
    scored.into_iter().map(|(p, _)| p).collect()
}

// =========================================================================
// Layer 4: Decision Memory
// =========================================================================

async fn layer_decision_memory(db: &BrainDb, query: &str) -> Vec<DecisionEntry> {
    let q = query.to_string();
    db.with_conn(move |conn| {
        let mut stmt = conn
            .prepare(
                "SELECT title, summary, content, created_at FROM nodes
                 WHERE (node_type = 'decision' OR cognitive_type = 'decision')
                 AND (title LIKE '%' || ?1 || '%' OR content LIKE '%' || ?1 || '%')
                 ORDER BY created_at DESC
                 LIMIT 5",
            )
            .map_err(|e| BrainError::Database(e.to_string()))?;

        let search_term = q.split_whitespace().take(3).collect::<Vec<_>>().join(" ");
        let rows = stmt
            .query_map(params![search_term], |row| {
                let title: String = row.get(0)?;
                let summary: String = row.get(1)?;
                let content: String = row.get(2)?;
                let created_at: String = row.get(3)?;
                Ok(DecisionEntry {
                    title,
                    summary: if !summary.is_empty() && summary.len() > 10 {
                        summary
                    } else {
                        compress_content_sync(&content, 200)
                    },
                    created_at,
                })
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
    .await
    .unwrap_or_default()
}

fn compress_content_sync(content: &str, max: usize) -> String {
    let cleaned = content.trim().replace('\n', " ").replace("  ", " ");
    if cleaned.chars().count() <= max {
        cleaned
    } else {
        let truncated: String = cleaned.chars().take(max).collect();
        format!("{}...", truncated)
    }
}

// =========================================================================
// Layer 5: Warnings (past mistakes, known pitfalls)
// =========================================================================

async fn layer_warnings(db: &BrainDb, query: &str) -> Vec<WarningEntry> {
    let q = query.to_string();
    db.with_conn(move |conn| {
        // Warnings come from: contradiction nodes, nodes with "warning"/"mistake"
        // in tags, and nodes with cognitive_type = "contradiction"
        let mut stmt = conn
            .prepare(
                "SELECT title, summary, content, node_type FROM nodes
                 WHERE (
                     node_type = 'contradiction'
                     OR cognitive_type = 'contradiction'
                     OR tags LIKE '%warning%'
                     OR tags LIKE '%mistake%'
                     OR tags LIKE '%gotcha%'
                     OR tags LIKE '%bug%'
                 )
                 AND (title LIKE '%' || ?1 || '%' OR content LIKE '%' || ?1 || '%')
                 ORDER BY quality_score DESC
                 LIMIT 4",
            )
            .map_err(|e| BrainError::Database(e.to_string()))?;

        let search_term = q.split_whitespace().take(3).collect::<Vec<_>>().join(" ");
        let rows = stmt
            .query_map(params![search_term], |row| {
                let title: String = row.get(0)?;
                let summary: String = row.get(1)?;
                let content: String = row.get(2)?;
                let node_type: String = row.get(3)?;
                Ok(WarningEntry {
                    title,
                    warning: if !summary.is_empty() && summary.len() > 10 {
                        summary
                    } else {
                        compress_content_sync(&content, 180)
                    },
                    severity: if node_type == "contradiction" {
                        "high".to_string()
                    } else {
                        "medium".to_string()
                    },
                })
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
    .await
    .unwrap_or_default()
}

// =========================================================================
// Layer 6: Predictions
// =========================================================================

async fn layer_predictions(db: &BrainDb, query: &str) -> Vec<PredictionEntry> {
    let q = query.to_string();
    db.with_conn(move |conn| {
        let mut stmt = conn
            .prepare(
                "SELECT title, content, confidence FROM nodes
                 WHERE node_type = 'prediction'
                   AND (title LIKE '%' || ?1 || '%' OR content LIKE '%' || ?1 || '%')
                 ORDER BY created_at DESC
                 LIMIT 2",
            )
            .map_err(|e| BrainError::Database(e.to_string()))?;

        let search_term = q.split_whitespace().take(3).collect::<Vec<_>>().join(" ");
        let rows = stmt
            .query_map(params![search_term], |row| {
                let title: String = row.get(0)?;
                let content: String = row.get(1)?;
                let confidence: f32 = row.get::<_, f32>(2).unwrap_or(0.5);
                Ok(PredictionEntry {
                    prediction: if !title.is_empty() {
                        title
                    } else {
                        compress_content_sync(&content, 120)
                    },
                    confidence,
                    timeframe: "upcoming".to_string(),
                })
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
    .await
    .unwrap_or_default()
}

// =========================================================================
// Token budget enforcement
// =========================================================================

fn enforce_token_budget(bundle: &mut ContextBundle) {
    truncate_rules(&mut bundle.compiled_rules, BUDGET_RULES);
    truncate_knowledge(&mut bundle.knowledge_nodes, BUDGET_KNOWLEDGE);
    truncate_patterns(&mut bundle.work_patterns, BUDGET_PATTERNS);
    truncate_decisions(&mut bundle.decisions, BUDGET_DECISIONS);
    truncate_warnings(&mut bundle.warnings, BUDGET_WARNINGS);
    truncate_predictions(&mut bundle.predictions, BUDGET_PREDICTIONS);
}

fn truncate_rules(rules: &mut Vec<MatchedRule>, budget_tokens: usize) {
    let budget_chars = budget_tokens * CHARS_PER_TOKEN;
    let mut used = 0;
    let mut keep = 0;
    for r in rules.iter() {
        let entry_chars = r.action.len() + r.condition.len() + 30;
        if used + entry_chars > budget_chars {
            break;
        }
        used += entry_chars;
        keep += 1;
    }
    rules.truncate(keep.max(1).min(rules.len()));
}

fn truncate_knowledge(nodes: &mut Vec<CompressedNode>, budget_tokens: usize) {
    let budget_chars = budget_tokens * CHARS_PER_TOKEN;
    let mut used = 0;
    let mut keep = 0;
    for n in nodes.iter() {
        let entry_chars = n.title.len() + n.summary.len() + n.domain.len() + 30;
        if used + entry_chars > budget_chars {
            break;
        }
        used += entry_chars;
        keep += 1;
    }
    nodes.truncate(keep.max(1).min(nodes.len()));
}

fn truncate_patterns(patterns: &mut Vec<PatternEntry>, budget_tokens: usize) {
    let budget_chars = budget_tokens * CHARS_PER_TOKEN;
    let mut used = 0;
    let mut keep = 0;
    for p in patterns.iter() {
        let entry_chars = p.rule.len() + p.pattern_type.len() + 40;
        if used + entry_chars > budget_chars {
            break;
        }
        used += entry_chars;
        keep += 1;
    }
    patterns.truncate(keep.max(1).min(patterns.len()));
}

fn truncate_decisions(decisions: &mut Vec<DecisionEntry>, budget_tokens: usize) {
    let budget_chars = budget_tokens * CHARS_PER_TOKEN;
    let mut used = 0;
    let mut keep = 0;
    for d in decisions.iter() {
        let entry_chars = d.title.len() + d.summary.len() + 30;
        if used + entry_chars > budget_chars {
            break;
        }
        used += entry_chars;
        keep += 1;
    }
    decisions.truncate(keep.max(1).min(decisions.len()));
}

fn truncate_warnings(warnings: &mut Vec<WarningEntry>, budget_tokens: usize) {
    let budget_chars = budget_tokens * CHARS_PER_TOKEN;
    let mut used = 0;
    let mut keep = 0;
    for w in warnings.iter() {
        let entry_chars = w.title.len() + w.warning.len() + 20;
        if used + entry_chars > budget_chars {
            break;
        }
        used += entry_chars;
        keep += 1;
    }
    warnings.truncate(keep.max(1).min(warnings.len()));
}

fn truncate_predictions(predictions: &mut Vec<PredictionEntry>, budget_tokens: usize) {
    let budget_chars = budget_tokens * CHARS_PER_TOKEN;
    let mut used = 0;
    let mut keep = 0;
    for p in predictions.iter() {
        let entry_chars = p.prediction.len() + 40;
        if used + entry_chars > budget_chars {
            break;
        }
        used += entry_chars;
        keep += 1;
    }
    predictions.truncate(keep.max(1).min(predictions.len()));
}

fn count_bundle_chars(bundle: &ContextBundle) -> usize {
    let mut total = 0;
    for r in &bundle.compiled_rules {
        total += r.action.len() + r.condition.len() + 30;
    }
    for n in &bundle.knowledge_nodes {
        total += n.title.len() + n.summary.len() + n.domain.len() + 30;
    }
    for p in &bundle.work_patterns {
        total += p.rule.len() + p.pattern_type.len() + 40;
    }
    for d in &bundle.decisions {
        total += d.title.len() + d.summary.len() + 30;
    }
    for w in &bundle.warnings {
        total += w.title.len() + w.warning.len() + 20;
    }
    for p in &bundle.predictions {
        total += p.prediction.len() + 40;
    }
    total
}

// =========================================================================
// Render to structured markdown (the sidekick-context.md format)
// =========================================================================

pub fn render_sidekick_context(bundle: &ContextBundle) -> String {
    let now = chrono::Utc::now().to_rfc3339();
    let token_est = bundle.total_chars / CHARS_PER_TOKEN;
    let mut md = String::with_capacity(bundle.total_chars + 500);

    md.push_str("# NeuroVault Sidekick Context\n");
    md.push_str(&format!(
        "*Auto-generated. Updated: {} | ~{} tokens | {}ms*\n",
        now, token_est, bundle.generation_ms
    ));
    md.push_str(&format!("*Project: {} | Query basis: {}*\n\n", bundle.project, short(&bundle.query, 120)));

    // Layer 1: Rules
    if !bundle.compiled_rules.is_empty() {
        md.push_str("## Rules (deterministic, always follow)\n\n");
        for r in &bundle.compiled_rules {
            let icon = match r.rule_type.as_str() {
                "always" => "++",
                "never" => "!!",
                "prefer" => ">>",
                _ => "--",
            };
            md.push_str(&format!(
                "- [{}] {} ({:.0}% confidence)\n",
                icon,
                r.action,
                r.confidence * 100.0
            ));
        }
        md.push('\n');
    }

    // Layer 2: Knowledge
    if !bundle.knowledge_nodes.is_empty() {
        md.push_str("## Relevant Knowledge\n\n");
        for n in &bundle.knowledge_nodes {
            md.push_str(&format!(
                "- **[{}]** {} — {} _(score {:.2})_\n",
                n.domain, n.title, n.summary, n.score
            ));
        }
        md.push('\n');
    }

    // Layer 3: Work Patterns
    if !bundle.work_patterns.is_empty() {
        md.push_str("## Work Patterns\n\n");
        for p in &bundle.work_patterns {
            md.push_str(&format!(
                "- **{}** ({:.0}%, x{}): {}\n",
                p.pattern_type,
                p.confidence * 100.0,
                p.times_confirmed,
                p.rule
            ));
        }
        md.push('\n');
    }

    // Layer 4: Decisions
    if !bundle.decisions.is_empty() {
        md.push_str("## Past Decisions\n\n");
        for d in &bundle.decisions {
            md.push_str(&format!("- **{}**: {}\n", d.title, d.summary));
        }
        md.push('\n');
    }

    // Layer 5: Warnings
    if !bundle.warnings.is_empty() {
        md.push_str("## Warnings\n\n");
        for w in &bundle.warnings {
            let icon = if w.severity == "high" { "!!" } else { "!" };
            md.push_str(&format!("- [{}] **{}**: {}\n", icon, w.title, w.warning));
        }
        md.push('\n');
    }

    // Layer 6: Predictions
    if !bundle.predictions.is_empty() {
        md.push_str("## Predictions\n\n");
        for p in &bundle.predictions {
            md.push_str(&format!(
                "- {} ({:.0}% confidence, {})\n",
                p.prediction,
                p.confidence * 100.0,
                p.timeframe
            ));
        }
        md.push('\n');
    }

    md.push_str("---\n");
    md.push_str("_The brain compounds. Every conversation makes the next one smarter._\n");
    md
}

fn short(s: &str, max: usize) -> String {
    let s = s.trim().replace('\n', " ");
    if s.chars().count() <= max {
        s
    } else {
        let truncated: String = s.chars().take(max).collect();
        format!("{}...", truncated)
    }
}
