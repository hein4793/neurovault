use crate::db::BrainDb;
use crate::error::BrainError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CuriosityItem {
    pub topic: String,
    pub reason: String,
    pub priority: f64,
    pub source: String, // "gap", "popular", "edge_topic", "expansion"
}

/// Returns true if the topic looks like a real researchable topic, not a
/// folder name, file extension, or generic programming term that would
/// produce garbage research results.
fn is_researchable_topic(topic: &str) -> bool {
    let t = topic.trim().to_lowercase();

    // Too short — single letters or abbreviations ("c", "go", "js", "rs")
    // are ambiguous. Require at least 3 chars.
    if t.len() < 3 { return false; }

    // Must contain at least one alphabetic character
    if !t.chars().any(|c| c.is_alphabetic()) { return false; }

    // Reject if entirely a single generic word that's likely a directory
    // or programming construct, not a meaningful research topic.
    let junk_words: std::collections::HashSet<&str> = [
        // Common directory / build names
        "src", "lib", "bin", "obj", "dist", "build", "target", "debug",
        "release", "test", "tests", "spec", "docs", "doc", "assets",
        "public", "private", "internal", "vendor", "packages", "node_modules",
        "coverage", "output", "temp", "tmp", "cache", "data", "config",
        "scripts", "tools", "utils", "helpers", "common", "shared",
        "components", "hooks", "stores", "models", "views", "controllers",
        "services", "routes", "middleware", "types", "interfaces",
        "library", "module", "package", "crate", "project", "workspace",
        "schemas", "locales",
        // Dotfile / tool directories
        ".vscode", ".git", ".github", ".idea",
        // Programming language keywords / constructs
        "function", "class", "struct", "enum", "trait", "impl", "const",
        "static", "async", "await", "return", "import", "export",
        "default", "index", "main", "app", "run", "init", "setup",
        "error", "result", "option", "string", "number", "boolean",
        "true", "false", "null", "none", "self", "super", "mod",
        // File extensions that leak through
        "rs", "ts", "tsx", "js", "jsx", "py", "css", "html", "json",
        "toml", "yaml", "yml", "xml", "sql", "md", "txt",
        // Package managers / tools
        "npm", "npx", "yarn", "pnpm", "pip", "cargo",
        // Generic noise / brain-internal terms
        "general", "other", "misc", "unknown", "untitled", "todo",
        "readme", "changelog", "license", "makefile", "dockerfile",
        "file-import", "chat", "isolated_nodes", "pattern", "reference",
        "research", "rich",
    ].iter().copied().collect();

    // Single-word check
    let word_count = t.split(|c: char| c == ' ' || c == '-' || c == '_')
        .filter(|w| !w.is_empty())
        .count();

    if word_count <= 1 && junk_words.contains(t.as_str()) {
        return false;
    }

    // Reject topics that look like file paths or dot-separated names
    if t.contains('/') || t.contains('\\') || t.contains("..") {
        return false;
    }

    // Reject camelCase or PascalCase single words (likely code identifiers)
    // e.g. "useState", "BrainDb", "AppConfig"
    // Note: check the original (not lowercased) string for case patterns.
    if word_count <= 1 {
        let original = topic.trim();
        let orig_has_mid_uppercase = original.chars().skip(1).any(|c| c.is_uppercase());
        if orig_has_mid_uppercase {
            return false;
        }
    }

    true
}

/// Generate a curiosity queue based on knowledge gaps and user interests
pub async fn generate_curiosity_queue(db: &BrainDb) -> Result<Vec<CuriosityItem>, BrainError> {
    let mut items: Vec<CuriosityItem> = Vec::new();

    // 1. From knowledge gaps (already uses aggregates)
    let gaps = crate::learning::gaps::detect_gaps(db).await.unwrap_or_default();
    for gap in gaps.iter().take(5) {
        if is_researchable_topic(&gap.topic) {
            items.push(CuriosityItem {
                topic: gap.topic.clone(),
                reason: gap.reason.clone(),
                priority: gap.priority,
                source: "gap".to_string(),
            });
        }
    }

    // 2. Most accessed topics via aggregate
    struct TopicAccess { topic: String, total_access: u64 }
    let popular: Vec<TopicAccess> = db.with_conn(|conn| -> Result<Vec<TopicAccess>, BrainError> {
        let mut stmt = conn.prepare(
            "SELECT topic, SUM(access_count) AS total_access FROM nodes \
             WHERE topic != '' \
             AND source_type NOT IN ('file', 'chat_history') \
             GROUP BY topic ORDER BY total_access DESC LIMIT 15"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok(TopicAccess {
                topic: row.get(0)?,
                total_access: row.get(1)?,
            })
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(n) = r { result.push(n); } }
        Ok(result)
    }).await?;

    let mut popular_added = 0;
    for ta in &popular {
        if popular_added >= 5 { break; }
        if !is_researchable_topic(&ta.topic) {
            log::debug!("curiosity: skipping junk popular topic '{}'", ta.topic);
            continue;
        }
        items.push(CuriosityItem {
            topic: format!("{} (advanced)", ta.topic),
            reason: format!("High interest topic ({} views) - expand depth", ta.total_access),
            priority: 0.5 + (ta.total_access as f64 / 100.0).min(0.4),
            source: "popular".to_string(),
        });
        popular_added += 1;
    }

    // 3. Frequent tags that aren't primary topics — use aggregate
    struct TopicName { topic: String }
    let topics: Vec<TopicName> = db.with_conn(|conn| -> Result<Vec<TopicName>, BrainError> {
        let mut stmt = conn.prepare(
            "SELECT topic FROM nodes WHERE topic != '' GROUP BY topic LIMIT 200"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            Ok(TopicName { topic: row.get(0)? })
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(n) = r { result.push(n); } }
        Ok(result)
    }).await?;
    let topic_set: std::collections::HashSet<String> = topics.iter().map(|t| t.topic.clone()).collect();

    // Sample tags from recent high-quality nodes instead of ALL nodes
    struct TagNode { tags: Vec<String> }
    let tag_sample: Vec<TagNode> = db.with_conn(|conn| -> Result<Vec<TagNode>, BrainError> {
        let mut stmt = conn.prepare(
            "SELECT tags FROM nodes WHERE tags != '[]' ORDER BY quality_score DESC LIMIT 2000"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt.query_map([], |row| {
            let tags_json: String = row.get(0)?;
            Ok(TagNode {
                tags: serde_json::from_str(&tags_json).unwrap_or_default(),
            })
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for r in rows { if let Ok(n) = r { result.push(n); } }
        Ok(result)
    }).await?;

    let mut tag_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for node in &tag_sample {
        for tag in &node.tags {
            if !topic_set.contains(tag) && !tag.is_empty() && tag.len() > 2
                && is_researchable_topic(tag)
            {
                *tag_counts.entry(tag.clone()).or_insert(0) += 1;
            }
        }
    }
    let mut sorted_tags: Vec<_> = tag_counts.into_iter().filter(|(_, c)| *c >= 3).collect();
    sorted_tags.sort_by(|a, b| b.1.cmp(&a.1));

    for (tag, count) in sorted_tags.iter().take(5) {
        items.push(CuriosityItem {
            topic: tag.clone(),
            reason: format!("Mentioned {} times in tags but not a primary topic", count),
            priority: 0.3 + (*count as f64 / 20.0).min(0.3),
            source: "edge_topic".to_string(),
        });
    }

    items.sort_by(|a, b| b.priority.partial_cmp(&a.priority).unwrap_or(std::cmp::Ordering::Equal));
    items.truncate(15);

    Ok(items)
}
