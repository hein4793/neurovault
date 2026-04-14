use crate::db::models::*;
use crate::db::BrainDb;
use crate::error::BrainError;
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub async fn ingest_url(
    db: State<'_, Arc<BrainDb>>,
    url: String,
) -> Result<Vec<GraphNode>, BrainError> {
    let response = reqwest::get(&url)
        .await
        .map_err(|e| BrainError::Ingestion(e.to_string()))?;

    let html = response
        .text()
        .await
        .map_err(|e| BrainError::Ingestion(e.to_string()))?;

    let markdown = html2md::parse_html(&html);

    // Extract title from first heading or URL
    let title = markdown
        .lines()
        .find(|line| line.starts_with("# "))
        .map(|line| line.trim_start_matches("# ").to_string())
        .unwrap_or_else(|| url.clone());

    let chunks = chunk_text(&markdown, 1000);
    let mut created_nodes = Vec::new();

    for (i, chunk) in chunks.iter().enumerate() {
        let chunk_title = if chunks.len() > 1 {
            format!("{} (part {})", title, i + 1)
        } else {
            title.clone()
        };

        let node = db
            .create_node(CreateNodeInput {
                title: chunk_title,
                content: chunk.clone(),
                domain: "technology".to_string(),
                topic: extract_topic(&url),
                tags: vec!["web".to_string(), "ingested".to_string()],
                node_type: "reference".to_string(),
                source_type: "web".to_string(),
                source_url: Some(url.clone()),
            })
            .await?;

        created_nodes.push(node);
    }

    // Create edges between chunks of the same document
    if created_nodes.len() > 1 {
        for i in 0..created_nodes.len() - 1 {
            let _ = db
                .create_edge(CreateEdgeInput {
                    source_id: created_nodes[i].id.clone(),
                    target_id: created_nodes[i + 1].id.clone(),
                    relation_type: "part_of".to_string(),
                    evidence: "Sequential document chunks".to_string(),
                })
                .await;
        }
    }

    Ok(created_nodes)
}

#[tauri::command]
pub async fn ingest_text(
    db: State<'_, Arc<BrainDb>>,
    title: String,
    content: String,
    domain: String,
    topic: String,
) -> Result<GraphNode, BrainError> {
    db.create_node(CreateNodeInput {
        title,
        content,
        domain,
        topic,
        tags: vec!["manual".to_string()],
        node_type: "concept".to_string(),
        source_type: "manual".to_string(),
        source_url: None,
    })
    .await
}

#[tauri::command]
pub async fn import_claude_memory(
    db: State<'_, Arc<BrainDb>>,
) -> Result<Vec<GraphNode>, BrainError> {
    let home = dirs::home_dir().ok_or_else(|| BrainError::Io(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "Home directory not found",
    )))?;

    let mut all_nodes = Vec::new();

    // Import from Claude project memories
    let projects_dir = home.join(".claude").join("projects");
    if projects_dir.exists() {
        for entry in std::fs::read_dir(&projects_dir)? {
            let entry = entry?;
            let memory_dir = entry.path().join("memory");
            if memory_dir.exists() {
                let nodes = import_directory(&db, &memory_dir, "claude_memory").await?;
                all_nodes.extend(nodes);
            }
        }
    }

    // Import from UBS vault
    let vault_dir = home.join(".claude").join("ubs-vault");
    if vault_dir.exists() {
        let nodes = import_directory(&db, &vault_dir, "ubs_vault").await?;
        all_nodes.extend(nodes);
    }

    log::info!("Imported {} nodes from Claude memory", all_nodes.len());
    Ok(all_nodes)
}

async fn import_directory(
    db: &Arc<BrainDb>,
    dir: &std::path::Path,
    source_type: &str,
) -> Result<Vec<GraphNode>, BrainError> {
    let mut nodes = Vec::new();

    fn visit_dir(dir: &std::path::Path, files: &mut Vec<std::path::PathBuf>) -> std::io::Result<()> {
        if dir.is_dir() {
            for entry in std::fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    visit_dir(&path, files)?;
                } else if path.extension().map_or(false, |ext| ext == "md") {
                    files.push(path);
                }
            }
        }
        Ok(())
    }

    let mut files = Vec::new();
    visit_dir(dir, &mut files)?;

    for file_path in files {
        let content = match std::fs::read_to_string(&file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        if content.trim().is_empty() {
            continue;
        }

        let title = file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("untitled")
            .replace('_', " ")
            .replace('-', " ");

        // Determine domain from path
        let path_str = file_path.to_string_lossy().to_lowercase();
        let domain = if path_str.contains("architecture") || path_str.contains("tech") {
            "technology"
        } else if path_str.contains("business") || path_str.contains("billing") || path_str.contains("order") {
            "business"
        } else if path_str.contains("pattern") {
            "pattern"
        } else if path_str.contains("reference") || path_str.contains("glossary") {
            "reference"
        } else {
            "technology"
        };

        // Extract topic from file name
        let topic = file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("general")
            .replace('_', "-");

        match db
            .create_node(CreateNodeInput {
                title: title.clone(),
                content,
                domain: domain.to_string(),
                topic,
                tags: vec![source_type.to_string()],
                node_type: "reference".to_string(),
                source_type: source_type.to_string(),
                source_url: None,
            })
            .await
        {
            Ok(node) => nodes.push(node),
            Err(e) => log::warn!("Failed to import {}: {}", title, e),
        }
    }

    Ok(nodes)
}

fn chunk_text(text: &str, max_chunk_size: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current_chunk = String::new();

    for line in text.lines() {
        if current_chunk.len() + line.len() > max_chunk_size && !current_chunk.is_empty() {
            chunks.push(current_chunk.trim().to_string());
            current_chunk = String::new();
        }
        current_chunk.push_str(line);
        current_chunk.push('\n');
    }

    if !current_chunk.trim().is_empty() {
        chunks.push(current_chunk.trim().to_string());
    }

    if chunks.is_empty() {
        chunks.push(text.to_string());
    }

    chunks
}

/// Import all Claude Code chat history into the brain
#[tauri::command]
pub async fn import_chat_history(
    db: State<'_, Arc<BrainDb>>,
) -> Result<Vec<GraphNode>, BrainError> {
    let home = dirs::home_dir().ok_or_else(|| BrainError::Io(std::io::Error::new(
        std::io::ErrorKind::NotFound, "Home directory not found",
    )))?;

    let projects_dir = home.join(".claude").join("projects");
    if !projects_dir.exists() {
        return Ok(vec![]);
    }

    let mut all_nodes = Vec::new();

    for project_entry in std::fs::read_dir(&projects_dir)? {
        let project_entry = project_entry?;
        let project_path = project_entry.path();
        if !project_path.is_dir() { continue; }

        let project_name = project_path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .replace("C--Users-User-OneDrive-Desktop-", "")
            .replace("C--Users-User-", "home-")
            .replace('-', " ");

        // Find all .jsonl chat files (skip subagents)
        let mut chat_files: Vec<std::path::PathBuf> = Vec::new();
        for entry in std::fs::read_dir(&project_path)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "jsonl") {
                chat_files.push(path);
            }
        }

        for chat_file in chat_files {
            let content = match std::fs::read_to_string(&chat_file) {
                Ok(c) => c,
                Err(_) => continue,
            };

            // Parse JSONL - extract human messages and assistant summaries
            let mut conversation_chunks: Vec<String> = Vec::new();
            let mut current_chunk = String::new();
            let mut message_count = 0;

            for line in content.lines() {
                if line.trim().is_empty() { continue; }

                if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
                    let msg_type = json["type"].as_str().unwrap_or("");

                    // Extract human and assistant messages
                    if msg_type == "human" || msg_type == "assistant" {
                        let role = if msg_type == "human" { "User" } else { "Claude" };

                        // Get text content
                        if let Some(content_arr) = json["message"]["content"].as_array() {
                            for item in content_arr {
                                if let Some(text) = item["text"].as_str() {
                                    // Skip very short messages and tool calls
                                    if text.len() > 20 {
                                        let trimmed = if text.len() > 500 {
                                            format!("{}...", crate::truncate_str(text, 500))
                                        } else {
                                            text.to_string()
                                        };
                                        current_chunk += &format!("**{}**: {}\n\n", role, trimmed);
                                        message_count += 1;
                                    }
                                }
                            }
                        } else if let Some(text) = json["message"]["content"].as_str() {
                            if text.len() > 20 {
                                let trimmed = if text.len() > 500 {
                                    format!("{}...", crate::truncate_str(text, 500))
                                } else {
                                    text.to_string()
                                };
                                current_chunk += &format!("**{}**: {}\n\n", role, trimmed);
                                message_count += 1;
                            }
                        }
                    }

                    // Chunk every 6 messages
                    if message_count >= 6 {
                        if !current_chunk.trim().is_empty() {
                            conversation_chunks.push(current_chunk.clone());
                        }
                        current_chunk.clear();
                        message_count = 0;
                    }
                }
            }

            // Don't forget the last chunk
            if !current_chunk.trim().is_empty() {
                conversation_chunks.push(current_chunk);
            }

            // Create nodes from conversation chunks
            for (i, chunk) in conversation_chunks.iter().enumerate() {
                let session_id = chat_file.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .chars().take(8).collect::<String>();

                let title = format!("{} - Chat {} (part {})", project_name, session_id, i + 1);

                match db.create_node(CreateNodeInput {
                    title,
                    content: chunk.clone(),
                    domain: "personal".to_string(),
                    topic: project_name.to_lowercase().replace(' ', "-"),
                    tags: vec!["chat".to_string(), "history".to_string(), project_name.to_lowercase()],
                    node_type: "conversation".to_string(),
                    source_type: "chat_history".to_string(),
                    source_url: None,
                }).await {
                    Ok(node) => all_nodes.push(node),
                    Err(e) => log::warn!("Failed to import chat chunk: {}", e),
                }
            }
        }
    }

    log::info!("Imported {} chat history nodes", all_nodes.len());
    Ok(all_nodes)
}

fn extract_topic(url: &str) -> String {
    url.split('/')
        .filter(|s| !s.is_empty() && !s.contains("http") && !s.contains("www") && !s.contains('.'))
        .last()
        .unwrap_or("general")
        .to_string()
}

/// Research a topic by fetching documentation from multiple sources automatically
#[tauri::command]
pub async fn research_topic(
    db: State<'_, Arc<BrainDb>>,
    topic: String,
) -> Result<Vec<GraphNode>, BrainError> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("ClaudeBrain/0.1")
        .build()
        .map_err(|e| BrainError::Ingestion(e.to_string()))?;

    let mut all_nodes = Vec::new();
    let topic_lower = topic.to_lowercase().replace(' ', "-");
    let _topic_slug = topic.to_lowercase().replace(' ', "+");

    // Source URLs to try for documentation
    let urls = vec![
        // GitHub search for README
        format!("https://raw.githubusercontent.com/{0}/{0}/main/README.md", topic_lower),
        format!("https://raw.githubusercontent.com/{0}/{0}/master/README.md", topic_lower),
        // npm package page
        format!("https://registry.npmjs.org/{}/latest", topic_lower),
        // crates.io
        format!("https://crates.io/api/v1/crates/{}", topic_lower),
        // Wikipedia summary
        format!("https://en.wikipedia.org/api/rest_v1/page/summary/{}", topic.replace(' ', "_")),
        // Dev.to articles
        format!("https://dev.to/api/articles?tag={}&top=7&per_page=3", topic_lower),
    ];

    for url in &urls {
        match client.get(url).send().await {
            Ok(resp) if resp.status().is_success() => {
                let text = match resp.text().await {
                    Ok(t) if t.len() > 50 => t, // Skip tiny responses
                    _ => continue,
                };

                // Try to parse as JSON (APIs) or use as markdown/text
                let (title, content) = if url.contains("wikipedia") {
                    // Wikipedia API response
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                        let title = json["title"].as_str().unwrap_or(&topic).to_string();
                        let extract = json["extract"].as_str().unwrap_or("").to_string();
                        if extract.len() < 50 { continue; }
                        (format!("{} - Wikipedia", title), extract)
                    } else { continue; }
                } else if url.contains("npmjs") {
                    // npm registry response
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                        let name = json["name"].as_str().unwrap_or(&topic).to_string();
                        let desc = json["description"].as_str().unwrap_or("").to_string();
                        let readme = json["readme"].as_str().unwrap_or("").to_string();
                        let content = if readme.len() > 100 {
                            format!("# {}\n{}\n\n{}", name, desc, crate::truncate_str(&readme, 3000))
                        } else {
                            format!("# {}\n{}", name, desc)
                        };
                        if content.len() < 50 { continue; }
                        (format!("{} - npm", name), content)
                    } else { continue; }
                } else if url.contains("crates.io") {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                        let name = json["crate"]["name"].as_str().unwrap_or(&topic).to_string();
                        let desc = json["crate"]["description"].as_str().unwrap_or("").to_string();
                        let dl = json["crate"]["downloads"].as_u64().unwrap_or(0);
                        let content = format!("# {} (Rust crate)\n{}\nDownloads: {}", name, desc, dl);
                        (format!("{} - crates.io", name), content)
                    } else { continue; }
                } else if url.contains("dev.to") {
                    if let Ok(articles) = serde_json::from_str::<Vec<serde_json::Value>>(&text) {
                        let mut combined = format!("# {} - Dev.to Articles\n\n", topic);
                        for article in articles.iter().take(3) {
                            let title = article["title"].as_str().unwrap_or("Article");
                            let desc = article["description"].as_str().unwrap_or("");
                            let url = article["url"].as_str().unwrap_or("");
                            combined += &format!("## {}\n{}\nSource: {}\n\n", title, desc, url);
                        }
                        if combined.len() < 100 { continue; }
                        (format!("{} - Dev.to", topic), combined)
                    } else { continue; }
                } else {
                    // Raw markdown (GitHub README)
                    let md = if text.starts_with('<') {
                        html2md::parse_html(&text)
                    } else {
                        text.clone()
                    };
                    let title = md.lines()
                        .find(|l| l.starts_with("# "))
                        .map(|l| l.trim_start_matches("# ").to_string())
                        .unwrap_or(format!("{} - Documentation", topic));
                    (title, crate::truncate_str(&md, 5000).to_string())
                };

                // Chunk and ingest
                let chunks = chunk_text(&content, 1200);
                for (i, chunk) in chunks.iter().enumerate() {
                    let chunk_title = if chunks.len() > 1 {
                        format!("{} (part {})", title, i + 1)
                    } else {
                        title.clone()
                    };

                    match db.create_node(CreateNodeInput {
                        title: chunk_title,
                        content: chunk.clone(),
                        domain: "technology".to_string(),
                        topic: topic_lower.clone(),
                        tags: vec!["research".to_string(), "auto".to_string(), topic_lower.clone()],
                        node_type: "reference".to_string(),
                        source_type: "research".to_string(),
                        source_url: Some(url.clone()),
                    }).await {
                        Ok(node) => all_nodes.push(node),
                        Err(e) => log::warn!("Failed to create node: {}", e),
                    }
                }
            }
            _ => continue,
        }
    }

    // Also try to fetch the main website
    let main_url = format!("https://{}.dev", topic_lower);
    if let Ok(resp) = client.get(&main_url).send().await {
        if resp.status().is_success() {
            if let Ok(html) = resp.text().await {
                let md = html2md::parse_html(&html);
                if md.len() > 100 {
                    let title = md.lines()
                        .find(|l| l.starts_with("# "))
                        .map(|l| l.trim_start_matches("# ").to_string())
                        .unwrap_or(format!("{} - Official Site", topic));

                    for chunk in chunk_text(crate::truncate_str(&md, 4000), 1200).iter() {
                        if let Ok(node) = db.create_node(CreateNodeInput {
                            title: title.clone(),
                            content: chunk.clone(),
                            domain: "technology".to_string(),
                            topic: topic_lower.clone(),
                            tags: vec!["research".to_string(), "official".to_string()],
                            node_type: "reference".to_string(),
                            source_type: "research".to_string(),
                            source_url: Some(main_url.clone()),
                        }).await {
                            all_nodes.push(node);
                        }
                    }
                }
            }
        }
    }

    log::info!("Research '{}': found {} nodes from multiple sources", topic, all_nodes.len());
    Ok(all_nodes)
}

/// Batch research multiple topics at once
#[tauri::command]
pub async fn research_batch(
    db: State<'_, Arc<BrainDb>>,
    topics: Vec<String>,
) -> Result<Vec<GraphNode>, BrainError> {
    let mut all_nodes = Vec::new();
    for topic in topics {
        match research_topic_inner(&db, &topic).await {
            Ok(nodes) => all_nodes.extend(nodes),
            Err(e) => log::warn!("Failed to research '{}': {}", topic, e),
        }
    }
    Ok(all_nodes)
}

// Inner function for batch use (avoids State wrapper issues)
// Fetches from multiple sources and chunks content to produce 3-10 nodes per topic.
pub(crate) async fn research_topic_inner(
    db: &Arc<BrainDb>,
    topic: &str,
) -> Result<Vec<GraphNode>, BrainError> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("ClaudeBrain/0.1 (knowledge-graph; contact: github.com/neurovault)")
        .build()
        .map_err(|e| BrainError::Ingestion(e.to_string()))?;

    let topic_lower = topic.to_lowercase().replace(' ', "-");
    let topic_slug = topic.replace(' ', "_");
    let mut all_nodes = Vec::new();

    // ---------- Source 1: Wikipedia FULL article (mobile-html → text) ----------
    // Use the mobile-sections endpoint for the full article, not just the summary.
    let wiki_full_url = format!(
        "https://en.wikipedia.org/api/rest_v1/page/mobile-sections/{}",
        topic_slug
    );
    let wiki_source_url = format!("https://en.wikipedia.org/wiki/{}", topic_slug);

    if let Ok(resp) = client.get(&wiki_full_url).send().await {
        if resp.status().is_success() {
            if let Ok(text) = resp.text().await {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                    let title = json["lead"]["displaytitle"]
                        .as_str()
                        .or_else(|| json["lead"]["normalizedtitle"].as_str())
                        .unwrap_or(topic)
                        .to_string();
                    // Strip HTML tags for a clean title
                    let title_clean = title.replace("<i>", "").replace("</i>", "")
                        .replace("<b>", "").replace("</b>", "");

                    // Collect all sections into a combined markdown string
                    let mut full_text = String::new();

                    // Lead section
                    if let Some(lead_html) = json["lead"]["sections"]
                        .as_array()
                        .and_then(|arr| arr.first())
                        .and_then(|s| s["text"].as_str())
                    {
                        let lead_md = html2md::parse_html(lead_html);
                        if !lead_md.trim().is_empty() {
                            full_text += &format!("# {}\n\n{}\n\n", title_clean, lead_md);
                        }
                    }

                    // Remaining sections
                    if let Some(sections) = json["remaining"]["sections"].as_array() {
                        for section in sections {
                            let heading = section["line"].as_str().unwrap_or("");
                            // Skip non-content sections
                            let skip = ["References", "External links", "See also",
                                        "Notes", "Further reading", "Bibliography",
                                        "Sources", "Citations"];
                            if skip.iter().any(|s| s.eq_ignore_ascii_case(heading)) {
                                continue;
                            }
                            if let Some(html) = section["text"].as_str() {
                                let md = html2md::parse_html(html);
                                if md.trim().len() > 30 {
                                    let level = section["toclevel"].as_u64().unwrap_or(2);
                                    let hashes = "#".repeat(level.min(4) as usize + 1);
                                    full_text += &format!("{} {}\n\n{}\n\n", hashes, heading, md);
                                }
                            }
                        }
                    }

                    // Chunk the combined text (cap at 8000 chars to stay reasonable)
                    if full_text.len() > 100 {
                        let capped = crate::truncate_str(&full_text, 8000);
                        let chunks = chunk_text(capped, 1200);
                        log::info!(
                            "research_topic_inner '{}': Wikipedia returned {} chars -> {} chunks",
                            topic, full_text.len(), chunks.len()
                        );
                        for (i, chunk) in chunks.iter().enumerate() {
                            let chunk_title = if chunks.len() > 1 {
                                format!("{} - Wikipedia (part {})", title_clean, i + 1)
                            } else {
                                format!("{} - Wikipedia", title_clean)
                            };
                            match db.create_node(CreateNodeInput {
                                title: chunk_title,
                                content: chunk.clone(),
                                domain: "technology".to_string(),
                                topic: topic_lower.clone(),
                                tags: vec!["research".to_string(), "wikipedia".to_string()],
                                node_type: "concept".to_string(),
                                source_type: "research".to_string(),
                                source_url: Some(wiki_source_url.clone()),
                            }).await {
                                Ok(node) => all_nodes.push(node),
                                Err(e) => log::warn!("research wiki chunk {}: {}", i, e),
                            }
                        }
                    }
                }
            }
        }
    }

    // Fallback: if full article failed, try the summary endpoint
    if all_nodes.is_empty() {
        let wiki_summary_url = format!(
            "https://en.wikipedia.org/api/rest_v1/page/summary/{}",
            topic_slug
        );
        if let Ok(resp) = client.get(&wiki_summary_url).send().await {
            if resp.status().is_success() {
                if let Ok(text) = resp.text().await {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                        let title = json["title"].as_str().unwrap_or(topic).to_string();
                        let extract = json["extract"].as_str().unwrap_or("").to_string();
                        if extract.len() > 30 {
                            if let Ok(node) = db.create_node(CreateNodeInput {
                                title: format!("{} - Overview", title),
                                content: extract,
                                domain: "technology".to_string(),
                                topic: topic_lower.clone(),
                                tags: vec!["research".to_string(), "wikipedia".to_string()],
                                node_type: "concept".to_string(),
                                source_type: "research".to_string(),
                                source_url: Some(wiki_source_url.clone()),
                            }).await {
                                all_nodes.push(node);
                            }
                        }
                    }
                }
            }
        }
    }

    // ---------- Source 2: Dev.to articles ----------
    let devto_url = format!(
        "https://dev.to/api/articles?tag={}&top=7&per_page=3",
        topic_lower
    );
    if let Ok(resp) = client.get(&devto_url).send().await {
        if resp.status().is_success() {
            if let Ok(text) = resp.text().await {
                if let Ok(articles) = serde_json::from_str::<Vec<serde_json::Value>>(&text) {
                    for article in articles.iter().take(3) {
                        let art_title = article["title"].as_str().unwrap_or("Article");
                        let desc = article["description"].as_str().unwrap_or("");
                        let art_url = article["url"].as_str().unwrap_or("");
                        let tags_str = article["tag_list"]
                            .as_array()
                            .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join(", "))
                            .unwrap_or_default();
                        let content = format!(
                            "# {}\n\n{}\n\nTags: {}\nSource: {}",
                            art_title, desc, tags_str, art_url
                        );
                        if content.len() < 60 { continue; }
                        match db.create_node(CreateNodeInput {
                            title: format!("{} (dev.to)", art_title),
                            content,
                            domain: "technology".to_string(),
                            topic: topic_lower.clone(),
                            tags: vec!["research".to_string(), "devto".to_string(), topic_lower.clone()],
                            node_type: "reference".to_string(),
                            source_type: "research".to_string(),
                            source_url: Some(art_url.to_string()),
                        }).await {
                            Ok(node) => all_nodes.push(node),
                            Err(e) => log::warn!("research devto node: {}", e),
                        }
                    }
                }
            }
        }
    }

    // ---------- Source 3: npm registry (for tech topics) ----------
    let npm_url = format!("https://registry.npmjs.org/{}/latest", topic_lower);
    if let Ok(resp) = client.get(&npm_url).send().await {
        if resp.status().is_success() {
            if let Ok(text) = resp.text().await {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                    let name = json["name"].as_str().unwrap_or(topic).to_string();
                    let desc = json["description"].as_str().unwrap_or("").to_string();
                    let readme = json["readme"].as_str().unwrap_or("").to_string();
                    let content = if readme.len() > 100 {
                        format!("# {} (npm)\n{}\n\n{}", name, desc, crate::truncate_str(&readme, 4000))
                    } else {
                        format!("# {} (npm)\n{}", name, desc)
                    };
                    if content.len() > 60 {
                        let chunks = chunk_text(&content, 1200);
                        for (i, chunk) in chunks.iter().enumerate() {
                            let chunk_title = if chunks.len() > 1 {
                                format!("{} - npm (part {})", name, i + 1)
                            } else {
                                format!("{} - npm", name)
                            };
                            match db.create_node(CreateNodeInput {
                                title: chunk_title,
                                content: chunk.clone(),
                                domain: "technology".to_string(),
                                topic: topic_lower.clone(),
                                tags: vec!["research".to_string(), "npm".to_string(), topic_lower.clone()],
                                node_type: "reference".to_string(),
                                source_type: "research".to_string(),
                                source_url: Some(npm_url.clone()),
                            }).await {
                                Ok(node) => all_nodes.push(node),
                                Err(e) => log::warn!("research npm chunk {}: {}", i, e),
                            }
                        }
                    }
                }
            }
        }
    }

    // ---------- Link sequential nodes from the same research session ----------
    if all_nodes.len() > 1 {
        for i in 0..all_nodes.len() - 1 {
            let _ = db.create_edge(CreateEdgeInput {
                source_id: all_nodes[i].id.clone(),
                target_id: all_nodes[i + 1].id.clone(),
                relation_type: "part_of".to_string(),
                evidence: format!("Research session: {}", topic),
            }).await;
        }
    }

    log::info!(
        "research_topic_inner '{}': created {} nodes from multiple sources",
        topic, all_nodes.len()
    );
    Ok(all_nodes)
}

/// Ingest files dropped by the user. Supports text-based source files and directories.
#[tauri::command]
pub async fn ingest_files(
    db: State<'_, Arc<BrainDb>>,
    paths: Vec<String>,
) -> Result<Vec<GraphNode>, BrainError> {
    let mut all_nodes = Vec::new();

    let text_extensions = [
        "md", "txt", "rs", "ts", "tsx", "js", "jsx", "py", "json", "toml",
        "yaml", "yml", "css", "html", "go", "java", "c", "cpp", "h", "hpp",
        "sql", "sh", "bat", "ps1", "xml", "csv", "cfg", "ini",
        "vue", "svelte", "astro", "prisma", "graphql", "proto",
        "pdf", "docx", "cs", "vb", "rb", "php", "swift", "kt",
    ];

    for path_str in &paths {
        let path = std::path::Path::new(path_str);
        if !path.exists() {
            log::warn!("File not found: {}", path_str);
            continue;
        }

        if path.is_dir() {
            let mut files = Vec::new();
            collect_ingest_files(path, &text_extensions, &mut files);
            let total = files.len();
            log::info!(
                "ingest_files: found {} eligible files in {} (cap {})",
                total, path_str, MAX_INGEST_FILES
            );
            for (i, file_path) in files.iter().enumerate() {
                if i > 0 && i % 100 == 0 {
                    log::info!("ingest_files: {}/{} files processed ({} nodes so far)", i, total, all_nodes.len());
                }
                match ingest_single_file(&db, file_path).await {
                    Ok(nodes) => all_nodes.extend(nodes),
                    Err(e) => log::warn!("Failed to ingest {:?}: {}", file_path, e),
                }
            }
        } else {
            match ingest_single_file(&db, path).await {
                Ok(nodes) => all_nodes.extend(nodes),
                Err(e) => log::warn!("Failed to ingest {}: {}", path_str, e),
            }
        }
    }

    // NOTE: auto_link_nodes removed from here — it's O(n²) and kills perf at scale.
    // The autonomy loop handles linking on a schedule instead.

    log::info!("File ingestion: {} nodes from {} paths", all_nodes.len(), paths.len());
    Ok(all_nodes)
}

/// Maximum file size for ingestion (500 KB). Larger files are usually
/// generated code, SQL dumps, or bundled output — not useful for the
/// brain and would choke the pipeline.
const MAX_INGEST_FILE_SIZE: u64 = 500_000;

/// Maximum total files per directory ingestion to prevent multi-hour hangs
/// on huge repos.
const MAX_INGEST_FILES: usize = 2000;

fn collect_ingest_files(dir: &std::path::Path, extensions: &[&str], out: &mut Vec<std::path::PathBuf>) {
    // Safety cap — don't walk 12K files
    if out.len() >= MAX_INGEST_FILES {
        return;
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if out.len() >= MAX_INGEST_FILES {
                return;
            }
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                // Skip directories that contain generated code, build
                // artefacts, caches, or IDE internals. These would flood
                // the brain with noise and cause multi-hour hangs.
                if name.starts_with('.')
                    || matches!(
                        name,
                        "node_modules"
                            | "target"
                            | "__pycache__"
                            | "dist"
                            | "build"
                            | "bin"
                            | "obj"
                            | "packages"
                            | "vendor"
                            | "venv"
                            | ".venv"
                            | ".next"
                            | ".nuxt"
                            | ".cache"
                            | ".idea"
                            | ".vscode"
                            | ".vs"
                            | "Debug"
                            | "Release"
                            | "x64"
                            | "x86"
                            | "TestResults"
                            | "coverage"
                    )
                {
                    continue;
                }
                collect_ingest_files(&path, extensions, out);
            } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if extensions.contains(&ext.to_lowercase().as_str()) {
                    // Skip files over 500 KB — likely generated
                    let too_big = std::fs::metadata(&path)
                        .map(|m| m.len() > MAX_INGEST_FILE_SIZE)
                        .unwrap_or(false);
                    if too_big {
                        log::debug!("Skipping large file: {:?}", path);
                        continue;
                    }
                    out.push(path);
                }
            }
        }
    }
}

/// Extract text from a PDF file and ingest as knowledge nodes.
async fn ingest_pdf(
    db: &Arc<BrainDb>,
    file_path: &std::path::Path,
    file_name: &str,
) -> Result<Vec<GraphNode>, BrainError> {
    let bytes = std::fs::read(file_path)
        .map_err(|e| BrainError::Ingestion(format!("Can't read PDF {:?}: {}", file_path, e)))?;

    let text = pdf_extract::extract_text_from_mem(&bytes)
        .map_err(|e| BrainError::Ingestion(format!("PDF extraction failed for {:?}: {}", file_path, e)))?;

    let text = text.trim().to_string();
    if text.len() < 20 {
        log::warn!("PDF {} extracted but too short ({} chars)", file_name, text.len());
        return Ok(Vec::new());
    }

    log::info!("PDF {} extracted {} chars of text", file_name, text.len());

    let topic = file_path.parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("documents")
        .to_string();

    let title_base = file_name.trim_end_matches(".pdf").to_string();
    let mut nodes = Vec::new();

    let chunks = chunk_text(&text, 1500);
    for (i, chunk) in chunks.iter().enumerate() {
        let chunk_title = if chunks.len() > 1 {
            format!("{} (part {})", title_base, i + 1)
        } else {
            title_base.clone()
        };
        match db.create_node(CreateNodeInput {
            title: chunk_title,
            content: chunk.clone(),
            domain: "business".to_string(),
            topic: topic.clone(),
            tags: vec!["pdf".to_string(), "document".to_string()],
            node_type: "reference".to_string(),
            source_type: "file".to_string(),
            source_url: Some(file_path.to_string_lossy().to_string()),
        }).await {
            Ok(node) => nodes.push(node),
            Err(e) => log::warn!("Failed PDF chunk {} of {}: {}", i, title_base, e),
        }
    }

    // Link sequential chunks
    for i in 1..nodes.len() {
        let _ = db.create_edge(CreateEdgeInput {
            source_id: nodes[i - 1].id.clone(),
            target_id: nodes[i].id.clone(),
            relation_type: "part_of".to_string(),
            evidence: format!("Part {} of {}", i + 1, title_base),
        }).await;
    }

    log::info!("PDF {}: created {} nodes", file_name, nodes.len());
    Ok(nodes)
}

/// Extract text from a DOCX file and ingest as knowledge nodes.
/// DOCX is a ZIP archive containing XML. We extract word/document.xml
/// and strip XML tags to get plain text.
async fn ingest_docx(
    db: &Arc<BrainDb>,
    file_path: &std::path::Path,
    file_name: &str,
) -> Result<Vec<GraphNode>, BrainError> {
    let file = std::fs::File::open(file_path)
        .map_err(|e| BrainError::Ingestion(format!("Can't open DOCX {:?}: {}", file_path, e)))?;

    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| BrainError::Ingestion(format!("Invalid DOCX (not a ZIP) {:?}: {}", file_path, e)))?;

    // Read word/document.xml — the main content file
    let mut text = String::new();
    if let Ok(mut doc_file) = archive.by_name("word/document.xml") {
        let mut xml = String::new();
        std::io::Read::read_to_string(&mut doc_file, &mut xml)
            .map_err(|e| BrainError::Ingestion(format!("Can't read document.xml: {}", e)))?;

        // Extract text between <w:t> tags and add paragraph breaks at </w:p>
        let mut in_text = false;
        let mut chars = xml.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '<' {
                let mut tag = String::new();
                while let Some(&tc) = chars.peek() {
                    chars.next();
                    if tc == '>' { break; }
                    tag.push(tc);
                }
                if tag.starts_with("w:t") && !tag.starts_with("w:tbl") {
                    in_text = true;
                } else if tag == "/w:t" {
                    in_text = false;
                } else if tag == "/w:p" {
                    text.push('\n');
                }
            } else if in_text {
                text.push(c);
            }
        }
    } else {
        return Err(BrainError::Ingestion(format!("No document.xml in DOCX {:?}", file_path)));
    }

    let text = text.trim().to_string();
    if text.len() < 20 {
        log::warn!("DOCX {} extracted but too short ({} chars)", file_name, text.len());
        return Ok(Vec::new());
    }

    log::info!("DOCX {} extracted {} chars of text", file_name, text.len());

    let topic = file_path.parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("documents")
        .to_string();

    let title_base = file_name.trim_end_matches(".docx").to_string();
    let mut nodes = Vec::new();

    let chunks = chunk_text(&text, 1500);
    for (i, chunk) in chunks.iter().enumerate() {
        let chunk_title = if chunks.len() > 1 {
            format!("{} (part {})", title_base, i + 1)
        } else {
            title_base.clone()
        };
        match db.create_node(CreateNodeInput {
            title: chunk_title,
            content: chunk.clone(),
            domain: "business".to_string(),
            topic: topic.clone(),
            tags: vec!["docx".to_string(), "document".to_string()],
            node_type: "reference".to_string(),
            source_type: "file".to_string(),
            source_url: Some(file_path.to_string_lossy().to_string()),
        }).await {
            Ok(node) => nodes.push(node),
            Err(e) => log::warn!("Failed DOCX chunk {} of {}: {}", i, title_base, e),
        }
    }

    // Link sequential chunks
    for i in 1..nodes.len() {
        let _ = db.create_edge(CreateEdgeInput {
            source_id: nodes[i - 1].id.clone(),
            target_id: nodes[i].id.clone(),
            relation_type: "part_of".to_string(),
            evidence: format!("Part {} of {}", i + 1, title_base),
        }).await;
    }

    log::info!("DOCX {}: created {} nodes", file_name, nodes.len());
    Ok(nodes)
}

/// Supported text extensions for ingestion
const TEXT_EXTENSIONS: &[&str] = &[
    "md", "txt", "rs", "ts", "tsx", "js", "jsx", "py", "json", "toml",
    "yaml", "yml", "css", "html", "go", "java", "c", "cpp", "h", "hpp",
    "sql", "sh", "bat", "ps1", "xml", "csv", "cfg", "ini",
    "vue", "svelte", "astro", "prisma", "graphql", "proto", "log",
    "cs", "vb", "rb", "php", "swift", "kt", "scala", "r", "m",
];

async fn ingest_single_file(
    db: &Arc<BrainDb>,
    file_path: &std::path::Path,
) -> Result<Vec<GraphNode>, BrainError> {
    let file_name = file_path.file_name().and_then(|n| n.to_str()).unwrap_or("untitled");
    let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();

    // Handle PDF files
    if ext == "pdf" {
        return ingest_pdf(db, file_path, file_name).await;
    }

    // Handle DOCX files
    if ext == "docx" {
        return ingest_docx(db, file_path, file_name).await;
    }

    // Skip other binary files we can't handle
    let binary_skip = ["doc", "xlsx", "xls", "pptx", "ppt",
                        "zip", "tar", "gz", "rar", "7z", "exe", "dll", "so",
                        "png", "jpg", "jpeg", "gif", "bmp", "ico", "svg",
                        "mp3", "mp4", "avi", "mov", "wav", "ogg",
                        "woff", "woff2", "ttf", "eot", "otf",
                        "db", "sqlite", "bin", "dat", "lnk"];
    if binary_skip.contains(&ext.as_str()) {
        log::info!("Skipping binary file (unsupported format): {}", file_name);
        return Ok(Vec::new());
    }

    // Text files — read as string
    let content = match std::fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(e) => {
            log::warn!("Can't read {:?} as text (likely binary): {}", file_path, e);
            return Ok(Vec::new());
        }
    };

    if content.trim().is_empty() || content.len() < 20 {
        return Ok(Vec::new());
    }

    let ext = ext.as_str();

    let path_str = file_path.to_string_lossy().to_lowercase();
    let domain = if ["rs", "ts", "tsx", "js", "jsx", "py", "go", "java", "c", "cpp", "h"].contains(&ext) {
        "technology"
    } else if path_str.contains("business") || path_str.contains("billing") {
        "business"
    } else {
        "technology"
    };

    let topic = file_path.parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("general")
        .to_string();

    let title = file_name.to_string();
    let mut nodes = Vec::new();

    if content.len() > 2000 {
        let chunks = chunk_text(&content, 1500);
        for (i, chunk) in chunks.iter().enumerate() {
            let chunk_title = if chunks.len() > 1 {
                format!("{} (part {})", title, i + 1)
            } else {
                title.clone()
            };
            match db.create_node(CreateNodeInput {
                title: chunk_title,
                content: chunk.clone(),
                domain: domain.to_string(),
                topic: topic.clone(),
                tags: vec!["file-import".to_string(), ext.to_string()],
                node_type: "reference".to_string(),
                source_type: "file".to_string(),
                source_url: Some(file_path.to_string_lossy().to_string()),
            }).await {
                Ok(node) => nodes.push(node),
                Err(e) => log::warn!("Failed chunk {} of {}: {}", i, title, e),
            }
        }
        for i in 1..nodes.len() {
            let _ = db.create_edge(CreateEdgeInput {
                source_id: nodes[i - 1].id.clone(),
                target_id: nodes[i].id.clone(),
                relation_type: "part_of".to_string(),
                evidence: format!("Part {} of {}", i + 1, title),
            }).await;
        }
    } else {
        match db.create_node(CreateNodeInput {
            title,
            content,
            domain: domain.to_string(),
            topic,
            tags: vec!["file-import".to_string(), ext.to_string()],
            node_type: "reference".to_string(),
            source_type: "file".to_string(),
            source_url: Some(file_path.to_string_lossy().to_string()),
        }).await {
            Ok(node) => nodes.push(node),
            Err(e) => log::warn!("Failed to ingest {}: {}", file_name, e),
        }
    }

    Ok(nodes)
}

/// Phase 1.3 — Ingest a project directory by walking it, parsing every
/// supported source file, and creating one `code_snippet` node per
/// extracted function / struct / class. Supports Rust, TypeScript,
/// JavaScript, Python, Go.
///
/// `max_files` caps how many files we process per call so a huge repo
/// doesn't overwhelm the brain. Default 500.
#[tauri::command]
pub async fn ingest_project_directory(
    db: State<'_, Arc<BrainDb>>,
    project_path: String,
    max_files: Option<usize>,
) -> Result<Vec<GraphNode>, BrainError> {
    let cap = max_files.unwrap_or(500);
    crate::ingestion::code::ingest_project_directory(&db, &project_path, cap).await
}
