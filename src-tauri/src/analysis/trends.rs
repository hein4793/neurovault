use crate::db::BrainDb;
use crate::error::BrainError;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainIqBreakdown {
    pub quality: f64,
    pub connectivity: f64,
    pub freshness: f64,
    pub diversity: f64,
    pub coverage: f64,
    pub volume: f64,
    pub depth: f64,
    pub cross_domain: f64,
    pub semantic: f64,
    pub research_ratio: f64,
    pub coherence: f64,
    pub high_quality_pct: f64,
    #[serde(default)]
    pub self_improvement_velocity: f64,
    #[serde(default)]
    pub prediction_accuracy: f64,
    #[serde(default)]
    pub novel_insight_rate: f64,
    #[serde(default)]
    pub autonomy_independence: f64,
    #[serde(default)]
    pub user_model_accuracy: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendReport {
    pub growth_by_day: Vec<DayCount>,
    pub domain_growth: Vec<DomainGrowth>,
    pub hot_topics: Vec<TopicHeat>,
    pub forgotten_topics: Vec<TopicHeat>,
    pub brain_iq: f64,
    pub iq_breakdown: BrainIqBreakdown,
    #[serde(default)]
    pub domain_iqs: Vec<crate::db::models::DomainIq>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayCount { pub date: String, pub count: u64 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainGrowth { pub domain: String, pub count: u64, pub recent_count: u64 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicHeat { pub topic: String, pub score: f64, pub node_count: u64 }

#[derive(Debug)]
#[allow(dead_code)]
struct LightNode {
    id: String,
    domain: String,
    topic: String,
    source_type: String,
    quality_score: f64,
    decay_score: f64,
    access_count: u64,
    created_at: String,
    has_embedding: bool,
}

#[derive(Debug)]
#[allow(dead_code)]
struct EmbeddingRow {
    id: String,
    domain: String,
    topic: String,
    embedding: Vec<f64>,
}

#[derive(Debug)]
struct LeanEdge { source_id: String, target_id: String }

pub async fn analyze_trends(db: &BrainDb) -> Result<TrendReport, BrainError> {
    let seven_days_ago = (chrono::Utc::now() - chrono::Duration::days(7)).to_rfc3339();

    // Load sampled nodes for IQ calc
    let nodes: Vec<LightNode> = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT n.id, n.domain, n.topic, n.source_type, n.quality_score, n.decay_score, \
             n.access_count, n.created_at, CASE WHEN e.node_id IS NOT NULL THEN 1 ELSE 0 END \
             FROM nodes n LEFT JOIN embeddings e ON e.node_id = n.id LIMIT 5000"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok(LightNode {
                id: row.get(0)?,
                domain: row.get(1)?,
                topic: row.get(2)?,
                source_type: row.get(3)?,
                quality_score: row.get(4)?,
                decay_score: row.get(5)?,
                access_count: row.get(6)?,
                created_at: row.get(7)?,
                has_embedding: row.get(8)?,
            })
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(n) = r { result.push(n); } }
        Ok(result)
    }).await?;

    let edges: Vec<LeanEdge> = db.with_conn(|conn| {
        let mut stmt = conn.prepare("SELECT source_id, target_id FROM edges LIMIT 50000")
            .map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok(LeanEdge { source_id: row.get(0)?, target_id: row.get(1)? })
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(e) = r { result.push(e); } }
        Ok(result)
    }).await?;

    // Growth by day
    let mut day_counts: HashMap<String, u64> = HashMap::new();
    for node in &nodes {
        let date = node.created_at.split('T').next().unwrap_or("unknown").to_string();
        *day_counts.entry(date).or_insert(0) += 1;
    }
    let mut growth_by_day: Vec<DayCount> = day_counts.into_iter().map(|(date, count)| DayCount { date, count }).collect();
    growth_by_day.sort_by(|a, b| a.date.cmp(&b.date));
    let len = growth_by_day.len();
    if len > 30 { growth_by_day = growth_by_day[len-30..].to_vec(); }

    // Domain growth
    let mut domain_total: HashMap<String, u64> = HashMap::new();
    let mut domain_recent: HashMap<String, u64> = HashMap::new();
    for node in &nodes {
        *domain_total.entry(node.domain.clone()).or_insert(0) += 1;
        if node.created_at > seven_days_ago {
            *domain_recent.entry(node.domain.clone()).or_insert(0) += 1;
        }
    }
    let domain_growth: Vec<DomainGrowth> = domain_total.iter().map(|(domain, count)| {
        DomainGrowth { domain: domain.clone(), count: *count, recent_count: domain_recent.get(domain).copied().unwrap_or(0) }
    }).collect();

    // Hot/forgotten topics
    let mut topic_access: HashMap<String, (u64, u64)> = HashMap::new();
    let mut topic_decay: HashMap<String, (f64, u64)> = HashMap::new();
    for node in &nodes {
        if !node.topic.is_empty() {
            let entry = topic_access.entry(node.topic.clone()).or_insert((0, 0));
            entry.0 += node.access_count; entry.1 += 1;
            let dentry = topic_decay.entry(node.topic.clone()).or_insert((0.0, 0));
            dentry.0 += node.decay_score; dentry.1 += 1;
        }
    }
    let mut hot_topics: Vec<TopicHeat> = topic_access.iter()
        .map(|(t, (a, c))| TopicHeat { topic: t.clone(), score: *a as f64, node_count: *c }).collect();
    hot_topics.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    hot_topics.truncate(10);

    let mut forgotten_topics: Vec<TopicHeat> = topic_decay.iter()
        .map(|(t, (d, c))| TopicHeat { topic: t.clone(), score: if *c > 0 { d / *c as f64 } else { 1.0 }, node_count: *c }).collect();
    forgotten_topics.sort_by(|a, b| a.score.partial_cmp(&b.score).unwrap_or(std::cmp::Ordering::Equal));
    forgotten_topics.truncate(10);

    // DB aggregates for accurate counts
    let (db_total_nodes, db_avg_quality, db_avg_decay) = db.with_conn(|conn| {
        conn.query_row(
            "SELECT COUNT(*), COALESCE(AVG(quality_score), 0), COALESCE(AVG(decay_score), 0) FROM nodes",
            [], |row| Ok((row.get::<_, f64>(0)?, row.get::<_, f64>(1)?, row.get::<_, f64>(2)?)),
        ).map_err(|e| BrainError::Database(e.to_string()))
    }).await?;

    let db_total_edges: f64 = db.with_conn(|conn| {
        conn.query_row("SELECT COUNT(*) FROM edges", [], |row| row.get::<_, f64>(0))
            .map_err(|e| BrainError::Database(e.to_string()))
    }).await?;

    let total_nodes = db_total_nodes;
    let total_edges = db_total_edges;

    // FOUNDATION TIER
    let quality_pts = db_avg_quality * 25.0;
    let connectivity_pts = if total_nodes > 0.0 { (total_edges / total_nodes).min(5.0) / 5.0 * 20.0 } else { 0.0 };
    let freshness_pts = db_avg_decay * 20.0;
    let diversity_pts = (domain_total.len() as f64 / 7.0).min(1.0) * 15.0;
    let emb_coverage = nodes.iter().filter(|n| n.has_embedding).count() as f64 / nodes.len().max(1) as f64;
    let coverage_pts = emb_coverage * 10.0;
    let volume_pts = (total_nodes.log10().max(0.0) / 5.7).min(1.0) * 10.0;

    // INTELLIGENCE TIER
    let topic_count = topic_access.len().max(1) as f64;
    let avg_depth = nodes.len() as f64 / topic_count;
    let depth_pts = (avg_depth / 8.0).min(1.0) * 20.0;

    let bridge_count = count_bridges(&nodes, &edges);
    let bridge_target = (total_nodes / 200.0).max(20.0).min(500.0);
    let cross_domain_pts = ((bridge_count as f64) / bridge_target).min(1.0) * 20.0;

    // Embedding sample for semantic density
    let emb_sample: Vec<EmbeddingRow> = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT n.id, n.domain, n.topic, e.vector, e.dimension \
             FROM nodes n INNER JOIN embeddings e ON e.node_id = n.id LIMIT 200"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let domain: String = row.get(1)?;
            let topic: String = row.get(2)?;
            let blob: Vec<u8> = row.get(3)?;
            let dim: usize = row.get(4)?;
            let embedding: Vec<f64> = blob.chunks_exact(8).take(dim)
                .map(|c| f64::from_le_bytes(c.try_into().unwrap_or([0u8; 8]))).collect();
            Ok(EmbeddingRow { id, domain, topic, embedding })
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(e) = r { result.push(e); } }
        Ok(result)
    }).await.unwrap_or_default();

    let semantic_density = compute_avg_edge_similarity(&emb_sample, &edges);
    let semantic_pts = semantic_density * 20.0;

    // Research ratio
    let source_counts: Vec<(String, u64)> = db.with_conn(|conn| {
        let mut stmt = conn.prepare("SELECT source_type, COUNT(*) FROM nodes GROUP BY source_type")
            .map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?)))
            .map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(s) = r { result.push(s); } }
        Ok(result)
    }).await.unwrap_or_default();

    let research_nodes = source_counts.iter()
        .filter(|(s, _)| matches!(s.as_str(), "research" | "web" | "manual" | "synthesis" | "project"))
        .map(|(_, c)| *c).sum::<u64>() as f64;
    let active_nodes = source_counts.iter().filter(|(s, _)| s != "file").map(|(_, c)| *c).sum::<u64>() as f64;
    let research_ratio = research_nodes / active_nodes.max(1.0);
    let research_ratio_pts = research_ratio * 20.0;

    let coherence = compute_topic_coherence(&emb_sample);
    let coherence_pts = coherence * 10.0;

    let hq_count = nodes.iter().filter(|n| n.quality_score > 0.7).count() as f64;
    let hq_pct = hq_count / total_nodes.max(1.0);
    let hq_pts = hq_pct * 10.0;

    // META-INTELLIGENCE TIER
    let synth_count: f64 = db.with_conn(|conn| {
        conn.query_row("SELECT COUNT(*) FROM nodes WHERE synthesized_by_brain = 1", [], |row| row.get::<_, f64>(0))
            .map_err(|e| BrainError::Database(e.to_string()))
    }).await.unwrap_or(0.0);
    let synth_pct = if total_nodes > 0.0 { synth_count / total_nodes } else { 0.0 };
    let self_improvement_pts = (synth_pct / 0.05).min(1.0) * 25.0;

    let (pred_avg_conf, pred_total) = db.with_conn(|conn| {
        conn.query_row(
            "SELECT COALESCE(AVG(confidence), 0.5), COUNT(*) FROM nodes WHERE node_type = 'prediction'",
            [], |row| Ok((row.get::<_, f64>(0)?, row.get::<_, u64>(1)?)),
        ).map_err(|e| BrainError::Database(e.to_string()))
    }).await.unwrap_or((0.5, 0));

    let prediction_accuracy_pts = if pred_total >= 3 {
        let lift = (pred_avg_conf - 0.5).max(0.0) * 2.0;
        lift * 25.0
    } else {
        (pred_total as f64 * 2.0).min(5.0)
    };

    let seven_days_ago_iso = (chrono::Utc::now() - chrono::Duration::days(7)).to_rfc3339();
    let recent_insights: f64 = db.with_conn(move |conn| {
        let sda = seven_days_ago_iso.clone();
        conn.query_row(
            "SELECT COUNT(*) FROM nodes WHERE node_type IN ('insight', 'hypothesis') AND created_at > ?1",
            params![sda], |row| row.get::<_, f64>(0),
        ).map_err(|e| BrainError::Database(e.to_string()))
    }).await.unwrap_or(0.0);
    let target_insights = (total_nodes / 1000.0 * 10.0).max(5.0);
    let novel_insight_pts = (recent_insights / target_insights).min(1.0) * 20.0;

    let seven_days_ago2 = (chrono::Utc::now() - chrono::Duration::days(7)).to_rfc3339();
    let (recent_total, recent_synth) = db.with_conn(move |conn| {
        let sda = seven_days_ago2.clone();
        let total: f64 = conn.query_row(
            "SELECT COUNT(*) FROM nodes WHERE created_at > ?1", params![sda], |row| row.get(0),
        ).unwrap_or(0.0);
        let synth: f64 = conn.query_row(
            "SELECT COUNT(*) FROM nodes WHERE synthesized_by_brain = 1 AND created_at > ?1",
            params![sda], |row| row.get(0),
        ).unwrap_or(0.0);
        Ok((total, synth))
    }).await.unwrap_or((0.0, 0.0));
    let independence_ratio = if recent_total > 0.0 { recent_synth / recent_total } else { 0.0 };
    let autonomy_independence_pts = (independence_ratio / 0.10).min(1.0) * 15.0;

    let (cog_avg, cog_sum_conf, cog_total) = db.with_conn(|conn| {
        conn.query_row(
            "SELECT COALESCE(AVG(confidence), 0), COALESCE(SUM(times_confirmed), 0), COUNT(*) FROM user_cognition",
            [], |row| Ok((row.get::<_, f64>(0)?, row.get::<_, f64>(1)?, row.get::<_, u64>(2)?)),
        ).map_err(|e| BrainError::Database(e.to_string()))
    }).await.unwrap_or((0.0, 0.0, 0));

    let user_model_pts = if cog_total > 0 {
        let confirm_factor = (cog_sum_conf + 1.0).log10().min(1.5) / 1.5;
        (cog_avg * confirm_factor) * 15.0
    } else { 0.0 };

    let iq_breakdown = BrainIqBreakdown {
        quality: quality_pts, connectivity: connectivity_pts, freshness: freshness_pts,
        diversity: diversity_pts, coverage: coverage_pts, volume: volume_pts,
        depth: depth_pts, cross_domain: cross_domain_pts, semantic: semantic_pts,
        research_ratio: research_ratio_pts, coherence: coherence_pts, high_quality_pct: hq_pts,
        self_improvement_velocity: self_improvement_pts, prediction_accuracy: prediction_accuracy_pts,
        novel_insight_rate: novel_insight_pts, autonomy_independence: autonomy_independence_pts,
        user_model_accuracy: user_model_pts,
    };

    let foundation = quality_pts + connectivity_pts + freshness_pts + diversity_pts + coverage_pts + volume_pts;
    let intelligence = depth_pts + cross_domain_pts + semantic_pts + research_ratio_pts + coherence_pts + hq_pts;
    let meta = self_improvement_pts + prediction_accuracy_pts + novel_insight_pts + autonomy_independence_pts + user_model_pts;
    let brain_iq = foundation + intelligence + meta;

    let domain_iqs = compute_domain_iqs(db, &domain_total).await.unwrap_or_default();

    Ok(TrendReport { growth_by_day, domain_growth, hot_topics, forgotten_topics, brain_iq, iq_breakdown, domain_iqs })
}

async fn compute_domain_iqs(db: &BrainDb, domain_totals: &HashMap<String, u64>) -> Result<Vec<crate::db::models::DomainIq>, BrainError> {
    let mut sorted: Vec<(String, u64)> = domain_totals.iter().map(|(d, c)| (d.clone(), *c)).filter(|(_, c)| *c >= 10).collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));
    sorted.truncate(5);

    let mut out = Vec::new();
    for (domain, count) in sorted {
        let domain_clone = domain.clone();
        let (avg_quality, top_topics) = db.with_conn(move |conn| {
            let avg: f64 = conn.query_row(
                "SELECT COALESCE(AVG(quality_score), 0) FROM nodes WHERE domain = ?1",
                params![domain_clone], |row| row.get(0),
            ).unwrap_or(0.0);

            let mut stmt = conn.prepare(
                "SELECT topic, COUNT(*) as c FROM nodes WHERE domain = ?1 AND topic != '' GROUP BY topic ORDER BY c DESC LIMIT 3"
            ).map_err(|e| BrainError::Database(e.to_string()))?;
            let topics: Vec<String> = stmt.query_map(params![domain_clone], |row| row.get::<_, String>(0))
                .map_err(|e| BrainError::Database(e.to_string()))?
                .filter_map(|r| r.ok()).collect();

            Ok((avg, topics))
        }).await?;

        let quality_pts = avg_quality * 40.0;
        let volume_pts = ((count as f64).log10().max(0.0) / 4.0).min(1.0) * 30.0;
        let avg_topic_depth = if !top_topics.is_empty() { (count as f64 / (top_topics.len().max(1) as f64) / 30.0).min(1.0) } else { 0.0 };
        let depth_pts = avg_topic_depth * 30.0;

        out.push(crate::db::models::DomainIq { domain, iq: quality_pts + volume_pts + depth_pts, node_count: count, avg_quality, top_topics });
    }
    Ok(out)
}

fn count_bridges(nodes: &[LightNode], edges: &[LeanEdge]) -> usize {
    let domain_map: HashMap<String, &str> = nodes.iter().map(|n| (n.id.clone(), n.domain.as_str())).collect();
    let mut neighbors: HashMap<String, Vec<String>> = HashMap::new();
    for edge in edges {
        neighbors.entry(edge.source_id.clone()).or_default().push(edge.target_id.clone());
        neighbors.entry(edge.target_id.clone()).or_default().push(edge.source_id.clone());
    }
    let mut bridge_count = 0;
    for (id, neighs) in &neighbors {
        let mut domains: HashSet<&str> = HashSet::new();
        if let Some(d) = domain_map.get(id) { domains.insert(d); }
        for nid in neighs { if let Some(d) = domain_map.get(nid) { domains.insert(d); } }
        if domains.len() >= 3 { bridge_count += 1; }
    }
    bridge_count
}

fn compute_avg_edge_similarity(emb_rows: &[EmbeddingRow], edges: &[LeanEdge]) -> f64 {
    let emb_map: HashMap<&str, &Vec<f64>> = emb_rows.iter()
        .filter(|n| !n.embedding.is_empty()).map(|n| (n.id.as_str(), &n.embedding)).collect();
    let mut total_sim = 0.0; let mut count = 0usize;
    let step = if edges.len() > 500 { edges.len() / 500 } else { 1 };
    for (i, edge) in edges.iter().enumerate() {
        if i % step != 0 { continue; } if count >= 500 { break; }
        if let (Some(a), Some(b)) = (emb_map.get(edge.source_id.as_str()), emb_map.get(edge.target_id.as_str())) {
            let sim = cosine_similarity(a, b);
            if sim > 0.0 { total_sim += sim; count += 1; }
        }
    }
    if count > 0 { total_sim / count as f64 } else { 0.0 }
}

fn compute_topic_coherence(emb_rows: &[EmbeddingRow]) -> f64 {
    let mut topic_embs: HashMap<&str, Vec<&Vec<f64>>> = HashMap::new();
    for row in emb_rows {
        if !row.topic.is_empty() && !row.embedding.is_empty() {
            topic_embs.entry(&row.topic).or_default().push(&row.embedding);
        }
    }
    let mut total = 0.0; let mut measured = 0;
    let mut topics: Vec<(&str, &Vec<&Vec<f64>>)> = topic_embs.iter().filter(|(_, e)| e.len() >= 2).map(|(t, e)| (*t, e)).collect();
    topics.sort_by(|a, b| b.1.len().cmp(&a.1.len()));
    for (_, embs) in topics.iter().take(10) {
        let mut sim_sum = 0.0; let mut pairs = 0;
        let limit = embs.len().min(5);
        for i in 0..limit { for j in (i+1)..limit { sim_sum += cosine_similarity(embs[i], embs[j]); pairs += 1; } }
        if pairs > 0 { total += sim_sum / pairs as f64; measured += 1; }
    }
    if measured > 0 { total / measured as f64 } else { 0.5 }
}

fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
    if a.len() != b.len() || a.is_empty() { return 0.0; }
    let dot: f64 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let ma: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let mb: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();
    if ma == 0.0 || mb == 0.0 { return 0.0; }
    (dot / (ma * mb)).clamp(0.0, 1.0)
}
