pub mod training;

use crate::db::BrainDb;
use crate::error::BrainError;
use rusqlite::params;
use std::path::Path;

/// Export brain data as JSON — streamed in batches to avoid OOM
pub async fn export_json(db: &BrainDb, path: &str) -> Result<u64, BrainError> {
    use std::io::Write;

    if let Some(parent) = Path::new(path).parent() {
        std::fs::create_dir_all(parent).map_err(BrainError::Io)?;
    }
    let file = std::fs::File::create(path).map_err(BrainError::Io)?;
    let mut w = std::io::BufWriter::new(file);

    write!(w, "{{\"exported_at\":\"{}\",\"nodes\":[", chrono::Utc::now().to_rfc3339())
        .map_err(|e| BrainError::Io(e))?;

    let mut offset = 0u64;
    let mut count = 0u64;
    let mut first = true;
    loop {
        let batch: Vec<serde_json::Value> = db.with_conn(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, title, content, summary, content_hash, domain, topic, tags, node_type, \
                 source_type, source_url, source_file, quality_score, visual_size, cluster_id, \
                 created_at, updated_at, accessed_at, access_count, decay_score \
                 FROM nodes LIMIT 500 OFFSET ?1"
            ).map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt.query_map(params![offset], |row| {
                Ok(serde_json::json!({
                    "id": row.get::<_, String>(0)?,
                    "title": row.get::<_, String>(1)?,
                    "content": row.get::<_, String>(2)?,
                    "summary": row.get::<_, String>(3)?,
                    "content_hash": row.get::<_, String>(4)?,
                    "domain": row.get::<_, String>(5)?,
                    "topic": row.get::<_, String>(6)?,
                    "tags": row.get::<_, String>(7)?,
                    "node_type": row.get::<_, String>(8)?,
                    "source_type": row.get::<_, String>(9)?,
                    "source_url": row.get::<_, Option<String>>(10)?,
                    "source_file": row.get::<_, Option<String>>(11)?,
                    "quality_score": row.get::<_, f64>(12)?,
                    "visual_size": row.get::<_, f64>(13)?,
                    "cluster_id": row.get::<_, Option<String>>(14)?,
                    "created_at": row.get::<_, String>(15)?,
                    "updated_at": row.get::<_, String>(16)?,
                    "accessed_at": row.get::<_, String>(17)?,
                    "access_count": row.get::<_, u64>(18)?,
                    "decay_score": row.get::<_, f64>(19)?,
                }))
            }).map_err(|e| BrainError::Database(e.to_string()))?;
            let mut result = Vec::new();
            for r in rows { if let Ok(v) = r { result.push(v); } }
            Ok(result)
        }).await?;

        if batch.is_empty() { break; }
        for node in &batch {
            if !first { write!(w, ",").map_err(|e| BrainError::Io(e))?; }
            serde_json::to_writer(&mut w, node).map_err(BrainError::Serialization)?;
            first = false;
            count += 1;
        }
        offset += 500;
    }

    write!(w, "],\"edges\":[").map_err(|e| BrainError::Io(e))?;

    offset = 0;
    first = true;
    loop {
        let batch: Vec<serde_json::Value> = db.with_conn(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT source_id, target_id, relation_type, strength, discovered_by, evidence, \
                 animated, created_at, traversal_count FROM edges LIMIT 500 OFFSET ?1"
            ).map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt.query_map(params![offset], |row| {
                Ok(serde_json::json!({
                    "source_id": row.get::<_, String>(0)?,
                    "target_id": row.get::<_, String>(1)?,
                    "relation_type": row.get::<_, String>(2)?,
                    "strength": row.get::<_, f64>(3)?,
                    "discovered_by": row.get::<_, String>(4)?,
                    "evidence": row.get::<_, String>(5)?,
                    "animated": row.get::<_, bool>(6)?,
                    "created_at": row.get::<_, String>(7)?,
                    "traversal_count": row.get::<_, u64>(8)?,
                }))
            }).map_err(|e| BrainError::Database(e.to_string()))?;
            let mut result = Vec::new();
            for r in rows { if let Ok(v) = r { result.push(v); } }
            Ok(result)
        }).await?;

        if batch.is_empty() { break; }
        for edge in &batch {
            if !first { write!(w, ",").map_err(|e| BrainError::Io(e))?; }
            serde_json::to_writer(&mut w, edge).map_err(BrainError::Serialization)?;
            first = false;
        }
        offset += 500;
    }

    write!(w, "]}}").map_err(|e| BrainError::Io(e))?;
    w.flush().map_err(BrainError::Io)?;
    log::info!("Exported {} nodes to JSON (streamed)", count);
    Ok(count)
}

/// Export top 5000 nodes as Markdown files
pub async fn export_markdown(db: &BrainDb, dir: &str) -> Result<u64, BrainError> {
    let base = Path::new(dir);
    std::fs::create_dir_all(base).map_err(BrainError::Io)?;

    let nodes: Vec<(String, String, String, String, String, String)> = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT title, content, domain, topic, node_type, tags FROM nodes ORDER BY quality_score DESC LIMIT 5000"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?))
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(n) = r { result.push(n); } }
        Ok(result)
    }).await?;

    let mut count = 0u64;
    for (title, content, domain, topic, node_type, tags_json) in &nodes {
        let tags: Vec<String> = serde_json::from_str(tags_json).unwrap_or_default();
        let domain_dir = base.join(domain);
        std::fs::create_dir_all(&domain_dir).map_err(BrainError::Io)?;
        let safe_title: String = title.chars().take(80)
            .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { '_' }).collect();
        let filepath = domain_dir.join(format!("{}.md", safe_title.trim()));
        let md = format!("# {}\n\n**Domain:** {} | **Topic:** {} | **Type:** {}\n**Tags:** {}\n\n---\n\n{}\n",
            title, domain, topic, node_type, tags.join(", "), content);
        std::fs::write(&filepath, &md).map_err(BrainError::Io)?;
        count += 1;
    }
    Ok(count)
}

/// Export a compact brain briefing for Claude sessions
pub async fn export_brain_briefing(db: &BrainDb, path: &str) -> Result<(), BrainError> {
    let trends = crate::analysis::trends::analyze_trends(db).await.ok();
    let gaps = crate::learning::gaps::detect_gaps(db).await.unwrap_or_default();
    let curiosity = crate::learning::curiosity::generate_curiosity_queue(db).await.unwrap_or_default();

    let (total_nodes, avg_quality) = db.with_conn(|conn| {
        conn.query_row("SELECT COUNT(*), COALESCE(AVG(quality_score), 0) FROM nodes", [], |row| Ok((row.get::<_, u64>(0)?, row.get::<_, f64>(1)?)))
            .map_err(|e| BrainError::Database(e.to_string()))
    }).await?;

    let total_edges: u64 = db.with_conn(|conn| {
        conn.query_row("SELECT COUNT(*) FROM edges", [], |row| row.get(0))
            .map_err(|e| BrainError::Database(e.to_string()))
    }).await?;

    let iq = trends.as_ref().map(|t| t.brain_iq).unwrap_or(0.0);

    let domains: Vec<(String, u64)> = db.with_conn(|conn| {
        let mut stmt = conn.prepare("SELECT domain, COUNT(*) FROM nodes GROUP BY domain")
            .map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(d) = r { result.push(d); } }
        Ok(result)
    }).await?;

    let strong_topics: Vec<(String, u64, f64)> = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT topic, COUNT(*) as c, AVG(quality_score) as avg_q FROM nodes \
             WHERE topic != '' GROUP BY topic HAVING c > 4 AND avg_q > 0.7 LIMIT 15"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(t) = r { result.push(t); } }
        Ok(result)
    }).await?;

    let seven_days_ago = (chrono::Utc::now() - chrono::Duration::days(7)).to_rfc3339();
    let recent: Vec<(String, String)> = db.with_conn(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT title, domain FROM nodes WHERE created_at > ?1 ORDER BY created_at DESC LIMIT 20"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map(params![seven_days_ago], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(r) = r { result.push(r); } }
        Ok(result)
    }).await?;

    let syntheses: Vec<(String, String)> = db.with_conn(|conn| {
        let mut stmt = conn.prepare("SELECT title, summary FROM nodes WHERE node_type = 'synthesis' LIMIT 10")
            .map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(s) = r { result.push(s); } }
        Ok(result)
    }).await.unwrap_or_default();

    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M UTC");
    let mut md = format!("# Brain Briefing\n*Updated: {} | IQ: {:.0}/200 | {} neurons | {} synapses | Avg quality: {:.0}%*\n\n",
        now, iq, total_nodes, total_edges, avg_quality * 100.0);

    md.push_str("## Knowledge Domains\n");
    for (d, c) in &domains { md.push_str(&format!("- **{}**: {} nodes\n", d, c)); }
    md.push('\n');

    if !strong_topics.is_empty() {
        md.push_str("## Strongest Areas\n");
        for (t, c, q) in &strong_topics { md.push_str(&format!("- **{}** ({} nodes, {:.0}% quality)\n", t, c, q * 100.0)); }
        md.push('\n');
    }
    if !recent.is_empty() {
        md.push_str("## Recent Learnings (7 days)\n");
        for (title, domain) in &recent { md.push_str(&format!("- [{}] {}\n", domain, title)); }
        md.push('\n');
    }
    if !gaps.is_empty() {
        md.push_str("## Knowledge Gaps\n");
        for g in gaps.iter().take(5) { md.push_str(&format!("- **{}** (priority {:.0}%): {}\n", g.topic, g.priority * 100.0, g.reason)); }
        md.push('\n');
    }
    if !curiosity.is_empty() {
        md.push_str("## What To Learn Next\n");
        for c in curiosity.iter().take(5) { md.push_str(&format!("- **{}**: {}\n", c.topic, c.reason)); }
        md.push('\n');
    }

    let profile = crate::user::profile::load_profile(db).await;
    if !profile.primary_languages.is_empty() || !profile.frameworks.is_empty() {
        md.push_str("## User Profile\n");
        if !profile.primary_languages.is_empty() { md.push_str(&format!("- **Languages**: {}\n", profile.primary_languages.join(", "))); }
        if !profile.frameworks.is_empty() { md.push_str(&format!("- **Frameworks**: {}\n", profile.frameworks.join(", "))); }
        if !profile.coding_patterns.is_empty() { md.push_str(&format!("- **Patterns**: {}\n", profile.coding_patterns.join(", "))); }
        md.push_str(&format!("- **Learning velocity**: {:.1} nodes/day\n", profile.learning_velocity));
        md.push('\n');
    }

    if !syntheses.is_empty() {
        md.push_str("## Brain Insights (Synthesized Conclusions)\n");
        for (t, s) in &syntheses { md.push_str(&format!("- **{}**: {}\n", t, s)); }
        md.push('\n');
    }

    md.push_str("## Quick Reference\n");
    md.push_str("- Full search: `~/.neurovault/export/nodes/{domain}/`\n");
    md.push_str("- JSON data: `~/.neurovault/export/brain-knowledge.json`\n");
    md.push_str("- Brain index: `~/.neurovault/export/brain-index.json`\n");

    if let Some(parent) = Path::new(path).parent() { std::fs::create_dir_all(parent).map_err(BrainError::Io)?; }
    std::fs::write(path, &md).map_err(BrainError::Io)?;
    log::info!("Brain briefing exported to {}", path);
    Ok(())
}

/// Export machine-readable brain index
pub async fn export_brain_index(db: &BrainDb, path: &str) -> Result<(), BrainError> {
    let trends = crate::analysis::trends::analyze_trends(db).await.ok();
    let total_nodes: u64 = db.with_conn(|conn| conn.query_row("SELECT COUNT(*) FROM nodes", [], |r| r.get(0)).map_err(|e| BrainError::Database(e.to_string()))).await?;

    let domains: Vec<(String, u64)> = db.with_conn(|conn| {
        let mut s = conn.prepare("SELECT domain, COUNT(*) FROM nodes GROUP BY domain").map_err(|e| BrainError::Database(e.to_string()))?;
        let r = s.query_map([], |row| Ok((row.get(0)?, row.get(1)?))).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new(); for row in r { if let Ok(d) = row { result.push(d); } } Ok(result)
    }).await?;
    let domain_map: std::collections::HashMap<String, u64> = domains.into_iter().collect();

    let top_topics: Vec<(String, u64, f64)> = db.with_conn(|conn| {
        let mut s = conn.prepare("SELECT topic, COUNT(*) as c, AVG(quality_score) FROM nodes WHERE topic != '' GROUP BY topic ORDER BY c DESC LIMIT 20")
            .map_err(|e| BrainError::Database(e.to_string()))?;
        let r = s.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new(); for row in r { if let Ok(t) = row { result.push(t); } } Ok(result)
    }).await?;

    let gaps = crate::learning::gaps::detect_gaps(db).await.unwrap_or_default();

    let seven_days_ago = (chrono::Utc::now() - chrono::Duration::days(7)).to_rfc3339();
    let recent: Vec<String> = db.with_conn(move |conn| {
        let mut s = conn.prepare("SELECT title FROM nodes WHERE created_at > ?1 ORDER BY created_at DESC LIMIT 10")
            .map_err(|e| BrainError::Database(e.to_string()))?;
        let r = s.query_map(params![seven_days_ago], |row| row.get(0)).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new(); for row in r { if let Ok(t) = row { result.push(t); } } Ok(result)
    }).await?;

    let strongest: Vec<serde_json::Value> = top_topics.iter()
        .filter(|(_, c, q)| *q > 0.7 && *c >= 5)
        .map(|(t, c, q)| serde_json::json!({ "topic": t, "nodes": c, "quality": (q * 100.0).round() / 100.0 })).collect();

    let index = serde_json::json!({
        "version": 1, "updated_at": chrono::Utc::now().to_rfc3339(),
        "iq_score": trends.as_ref().map(|t| (t.brain_iq * 10.0).round() / 10.0).unwrap_or(0.0),
        "total_nodes": total_nodes, "domains": domain_map,
        "top_topics": top_topics.iter().map(|(t, _, _)| t).collect::<Vec<_>>(),
        "knowledge_gaps": gaps.iter().take(10).map(|g| &g.topic).collect::<Vec<_>>(),
        "recent_learnings": recent, "strongest_areas": strongest,
    });

    let json = serde_json::to_string_pretty(&index).map_err(BrainError::Serialization)?;
    if let Some(parent) = Path::new(path).parent() { std::fs::create_dir_all(parent).map_err(BrainError::Io)?; }
    std::fs::write(path, &json).map_err(BrainError::Io)?;
    Ok(())
}

/// Export training data as JSONL
pub async fn export_training_data(db: &BrainDb, path: &str) -> Result<u64, BrainError> {
    use std::io::Write;
    if let Some(parent) = Path::new(path).parent() { std::fs::create_dir_all(parent).map_err(BrainError::Io)?; }
    let file = std::fs::File::create(path).map_err(BrainError::Io)?;
    let mut w = std::io::BufWriter::new(file);
    let mut count = 0u64;

    let nodes: Vec<serde_json::Value> = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT title, content, summary, domain, topic, quality_score, tags, source_type FROM nodes WHERE quality_score > 0.5 ORDER BY quality_score DESC LIMIT 5000"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok(serde_json::json!({ "type": "knowledge", "title": row.get::<_, String>(0)?, "content": row.get::<_, String>(1)?,
                "summary": row.get::<_, String>(2)?, "domain": row.get::<_, String>(3)?, "topic": row.get::<_, String>(4)?,
                "quality": row.get::<_, f64>(5)?, "tags": row.get::<_, String>(6)?, "source_type": row.get::<_, String>(7)? }))
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new(); for r in rows { if let Ok(v) = r { result.push(v); } } Ok(result)
    }).await?;

    for entry in &nodes {
        serde_json::to_writer(&mut w, entry).map_err(BrainError::Serialization)?;
        writeln!(w).map_err(BrainError::Io)?;
        count += 1;
    }

    let edges: Vec<serde_json::Value> = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT source_id, target_id, relation_type, strength, evidence FROM edges WHERE strength > 0.5 ORDER BY strength DESC LIMIT 5000"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok(serde_json::json!({ "type": "relationship", "source": row.get::<_, String>(0)?, "target": row.get::<_, String>(1)?,
                "relation": row.get::<_, String>(2)?, "strength": row.get::<_, f64>(3)?, "evidence": row.get::<_, String>(4)? }))
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new(); for r in rows { if let Ok(v) = r { result.push(v); } } Ok(result)
    }).await?;

    for entry in &edges { serde_json::to_writer(&mut w, entry).map_err(BrainError::Serialization)?; writeln!(w).map_err(BrainError::Io)?; count += 1; }
    w.flush().map_err(BrainError::Io)?;
    log::info!("Training data exported: {} entries", count);
    Ok(count)
}

/// Lightweight export — runs frequently (every 30 min).
/// Only updates the briefing + index (small files Claude Code reads).
pub async fn run_full_export(db: &BrainDb) -> Result<(), BrainError> {
    let export_dir = db.config.export_dir();
    let briefing_path = export_dir.join("brain-briefing.md");
    let index_path = export_dir.join("brain-index.json");

    // Always update briefing + index (tiny files, fast)
    match export_brain_briefing(db, &briefing_path.to_string_lossy()).await {
        Ok(()) => log::info!("Export: brain briefing updated"),
        Err(e) => log::warn!("Export briefing failed: {}", e),
    }
    match export_brain_index(db, &index_path.to_string_lossy()).await {
        Ok(()) => log::info!("Export: brain index updated"),
        Err(e) => log::warn!("Export index failed: {}", e),
    }

    // Heavy exports (JSON, markdown, training) — only once per week
    // Check a marker file for last full export time
    let marker_path = export_dir.join(".last_full_export");
    let should_do_full = match std::fs::read_to_string(&marker_path) {
        Ok(ts) => {
            if let Ok(last) = chrono::DateTime::parse_from_rfc3339(ts.trim()) {
                let elapsed = chrono::Utc::now() - last.with_timezone(&chrono::Utc);
                elapsed.num_days() >= 7
            } else {
                true
            }
        }
        Err(_) => true, // No marker = never exported
    };

    if should_do_full {
        log::info!("Export: running weekly full export (JSON + markdown + training)...");
        let json_path = export_dir.join("brain-knowledge.json");
        let training_path = export_dir.join("training-data.jsonl");

        match export_json(db, &json_path.to_string_lossy()).await {
            Ok(c) => log::info!("Export: {} nodes to JSON", c),
            Err(e) => log::warn!("Export JSON failed: {}", e),
        }
        match export_training_data(db, &training_path.to_string_lossy()).await {
            Ok(c) => log::info!("Export: {} training entries", c),
            Err(e) => log::warn!("Export training data failed: {}", e),
        }

        // Write marker
        let _ = std::fs::write(&marker_path, chrono::Utc::now().to_rfc3339());
        log::info!("Export: weekly full export complete");
    }

    Ok(())
}

pub async fn export_csv(db: &BrainDb, path: &str) -> Result<u64, BrainError> {
    use std::io::Write;
    if let Some(parent) = Path::new(path).parent() { std::fs::create_dir_all(parent).map_err(BrainError::Io)?; }
    let file = std::fs::File::create(path).map_err(BrainError::Io)?;
    let mut w = std::io::BufWriter::new(file);
    writeln!(w, "id,title,domain,topic,node_type,source_type,quality_score,tags,created_at").map_err(BrainError::Io)?;

    let mut offset = 0u64;
    let mut count = 0u64;
    loop {
        let batch: Vec<(String, String, String, String, String, String, f64, String, String)> = db.with_conn(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, title, domain, topic, node_type, source_type, quality_score, tags, created_at FROM nodes LIMIT 500 OFFSET ?1"
            ).map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt.query_map(params![offset], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?, row.get(7)?, row.get(8)?))
            }).map_err(|e| BrainError::Database(e.to_string()))?;
            let mut result = Vec::new(); for r in rows { if let Ok(n) = r { result.push(n); } } Ok(result)
        }).await?;
        if batch.is_empty() { break; }
        for (id, title, domain, topic, nt, st, qs, tags, ca) in &batch {
            let tags_display: Vec<String> = serde_json::from_str(tags).unwrap_or_default();
            writeln!(w, "\"{}\",\"{}\",\"{}\",\"{}\",\"{}\",\"{}\",{:.2},\"{}\",\"{}\"",
                id.replace('"', "\"\""), title.replace('"', "\"\""), domain, topic.replace('"', "\"\""),
                nt, st, qs, tags_display.join("; "), ca).map_err(BrainError::Io)?;
            count += 1;
        }
        offset += 500;
    }
    w.flush().map_err(BrainError::Io)?;
    Ok(count)
}
