use crate::db::BrainDb;
use crate::error::BrainError;
use rusqlite::params;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    pub top_domains: Vec<(String, f64)>,
    pub top_topics: Vec<(String, f64)>,
    pub primary_languages: Vec<String>,
    pub frameworks: Vec<String>,
    pub coding_patterns: Vec<String>,
    pub learning_velocity: f64,
    pub total_nodes: u64,
    pub total_interactions: u64,
    pub last_synthesized: String,
}

/// Load the current user profile from DB (or generate a default)
pub async fn load_profile(db: &BrainDb) -> UserProfile {
    let profile: Option<UserProfile> = db.with_conn(|conn| -> Result<Option<UserProfile>, BrainError> {
        let mut stmt = conn.prepare(
            "SELECT data FROM user_profile LIMIT 1"
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        let mut rows = stmt.query_map([], |row| {
            let data: String = row.get(0)?;
            Ok(data)
        }).map_err(|e| BrainError::Database(e.to_string()))?;
        match rows.next() {
            Some(Ok(data)) => {
                Ok(serde_json::from_str::<UserProfile>(&data).ok())
            }
            _ => Ok(None),
        }
    }).await.ok().and_then(|opt| opt);

    profile.unwrap_or_else(|| UserProfile {
        top_domains: Vec::new(),
        top_topics: Vec::new(),
        primary_languages: Vec::new(),
        frameworks: Vec::new(),
        coding_patterns: Vec::new(),
        learning_velocity: 0.0,
        total_nodes: 0,
        total_interactions: 0,
        last_synthesized: String::new(),
    })
}

/// Synthesize user profile from node metadata and interactions
pub async fn synthesize_profile(db: &BrainDb) -> Result<String, BrainError> {
    let now = chrono::Utc::now().to_rfc3339();
    let thirty_days_ago = (chrono::Utc::now() - chrono::Duration::days(30)).to_rfc3339();

    let (top_domains, top_topics, tag_strings, recent_count, total_nodes, total_interactions) =
        db.with_conn(move |conn| -> Result<(Vec<(String, f64)>, Vec<(String, f64)>, Vec<String>, u64, u64, u64), BrainError> {
            // Top domains by node count
            let mut stmt = conn.prepare(
                "SELECT domain, COUNT(*) as count FROM nodes GROUP BY domain ORDER BY count DESC LIMIT 10"
            ).map_err(|e| BrainError::Database(e.to_string()))?;
            let domains: Vec<(String, f64)> = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as f64))
            }).map_err(|e| BrainError::Database(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

            // Top topics by node count
            let mut stmt2 = conn.prepare(
                "SELECT topic, COUNT(*) as count FROM nodes WHERE topic != '' GROUP BY topic ORDER BY count DESC LIMIT 15"
            ).map_err(|e| BrainError::Database(e.to_string()))?;
            let topics: Vec<(String, f64)> = stmt2.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as f64))
            }).map_err(|e| BrainError::Database(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

            // Detect languages from file import tags
            let mut stmt3 = conn.prepare(
                "SELECT tags FROM nodes WHERE source_type = 'file' LIMIT 500"
            ).map_err(|e| BrainError::Database(e.to_string()))?;
            let tags: Vec<String> = stmt3.query_map([], |row| {
                row.get::<_, String>(0)
            }).map_err(|e| BrainError::Database(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

            // Recent node count (last 30 days)
            let recent: i64 = conn.query_row(
                "SELECT COUNT(*) FROM nodes WHERE created_at > ?1",
                params![thirty_days_ago],
                |row| row.get(0),
            ).unwrap_or(0);

            // Total nodes
            let total: i64 = conn.query_row(
                "SELECT COUNT(*) FROM nodes", [], |row| row.get(0),
            ).unwrap_or(0);

            // Total interactions
            let interactions: i64 = conn.query_row(
                "SELECT COUNT(*) FROM user_interaction", [], |row| row.get(0),
            ).unwrap_or(0);

            Ok((domains, topics, tags, recent as u64, total as u64, interactions as u64))
        }).await?;

    // Parse tag JSON arrays to detect languages and frameworks
    let lang_extensions = ["ts", "tsx", "js", "jsx", "py", "rs", "go", "java", "c", "cpp", "css", "html", "sql"];
    let framework_tags = ["react", "next", "tauri", "vue", "svelte", "express", "fastapi", "django", "tailwind", "prisma"];

    let mut lang_counts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    let mut fw_set: Vec<String> = Vec::new();

    for tag_json in &tag_strings {
        let parsed: Vec<String> = serde_json::from_str(tag_json).unwrap_or_default();
        for tag in parsed {
            let lower = tag.to_lowercase();
            if lang_extensions.contains(&lower.as_str()) {
                *lang_counts.entry(lower.clone()).or_insert(0) += 1;
            }
            if framework_tags.contains(&lower.as_str()) && !fw_set.contains(&tag) {
                fw_set.push(tag);
            }
        }
    }

    let mut lang_sorted: Vec<(String, u32)> = lang_counts.into_iter().collect();
    lang_sorted.sort_by(|a, b| b.1.cmp(&a.1));
    let primary_languages: Vec<String> = lang_sorted.into_iter().take(5).map(|(l, _)| l).collect();

    // Build coding patterns
    let mut patterns: Vec<String> = Vec::new();
    if primary_languages.contains(&"ts".to_string()) || primary_languages.contains(&"tsx".to_string()) {
        patterns.push("Uses TypeScript".to_string());
    }
    if primary_languages.contains(&"rs".to_string()) {
        patterns.push("Uses Rust".to_string());
    }
    if fw_set.iter().any(|f| f.to_lowercase().contains("react")) {
        patterns.push("React frontend".to_string());
    }
    if fw_set.iter().any(|f| f.to_lowercase().contains("tauri")) {
        patterns.push("Builds desktop apps with Tauri".to_string());
    }

    let learning_velocity = recent_count as f64 / 30.0;

    let profile = UserProfile {
        top_domains,
        top_topics,
        primary_languages,
        frameworks: fw_set,
        coding_patterns: patterns,
        learning_velocity,
        total_nodes,
        total_interactions,
        last_synthesized: now.clone(),
    };

    // Upsert profile
    let profile_json = serde_json::to_string(&profile)
        .map_err(BrainError::Serialization)?;
    let id = "profile:main".to_string();
    db.with_conn(move |conn| -> Result<(), BrainError> {
        conn.execute(
            "INSERT OR REPLACE INTO user_profile (id, data) VALUES (?1, ?2)",
            params![id, profile_json],
        ).map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(())
    }).await?;

    log::info!("User profile synthesized: {} domains, {} topics, {} languages",
        profile.top_domains.len(), profile.top_topics.len(), profile.primary_languages.len());

    Ok(format!("Profile updated: {} languages, {} frameworks, {:.1} nodes/day velocity",
        profile.primary_languages.len(), profile.frameworks.len(), profile.learning_velocity))
}
