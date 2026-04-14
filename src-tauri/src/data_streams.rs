//! Phase Omega Part V — Real-Time Data Streams
//!
//! Register and poll external data sources (RSS feeds, JSON APIs, file
//! watchers, webhooks) on configurable intervals. Each new item from a
//! stream becomes a knowledge node in the brain graph, enabling the brain
//! to absorb real-time information from the world.
//!
//! ## Stream types
//!
//! - `rss` — RSS/Atom feed polling
//! - `api_poll` — JSON API endpoint polling
//! - `file_watch` — local file change detection
//! - `webhook` — placeholder for future push-based ingestion
//!
//! ## Background poller
//!
//! `run_stream_poller` is spawned as a background task and checks all
//! enabled streams on their configured intervals.

use crate::db::models::CreateNodeInput;
use crate::db::BrainDb;
use crate::error::BrainError;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;

// =========================================================================
// DATA STRUCTURES
// =========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataStream {
    pub id: String,
    pub name: String,
    pub stream_type: String, // "rss", "webhook", "api_poll", "file_watch"
    pub url: String,
    pub poll_interval_mins: u64,
    pub last_polled: Option<String>,
    pub enabled: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamEvent {
    pub id: String,
    pub stream_id: String,
    pub title: String,
    pub content: String,
    pub source_url: Option<String>,
    pub created_at: String,
}

// =========================================================================
// STREAM MANAGEMENT
// =========================================================================

/// Register a new data stream.
pub async fn add_stream(
    db: &Arc<BrainDb>,
    name: String,
    stream_type: String,
    url: String,
    interval: u64,
) -> Result<DataStream, BrainError> {
    let id = format!("stream:{}", uuid::Uuid::now_v7());
    let now = chrono::Utc::now().to_rfc3339();
    let interval_val = if interval == 0 { 60 } else { interval };

    let stream = DataStream {
        id: id.clone(),
        name: name.clone(),
        stream_type: stream_type.clone(),
        url: url.clone(),
        poll_interval_mins: interval_val,
        last_polled: None,
        enabled: true,
        created_at: now.clone(),
    };

    db.with_conn(move |conn| {
        conn.execute(
            "INSERT INTO data_streams (id, name, stream_type, url, poll_interval_mins, last_polled, enabled, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, NULL, 1, ?6)",
            params![id, name, stream_type, url, interval_val, now],
        )
        .map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(())
    })
    .await?;

    log::info!("Data stream '{}' registered: {} ({})", stream.name, stream.url, stream.stream_type);
    Ok(stream)
}

/// List all registered data streams.
pub async fn get_streams(db: &Arc<BrainDb>) -> Result<Vec<DataStream>, BrainError> {
    db.with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT id, name, stream_type, url, poll_interval_mins, last_polled, enabled, created_at
                 FROM data_streams ORDER BY created_at DESC",
            )
            .map_err(|e| BrainError::Database(e.to_string()))?;

        let streams = stmt
            .query_map([], |row| {
                Ok(DataStream {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    stream_type: row.get(2)?,
                    url: row.get(3)?,
                    poll_interval_mins: row.get::<_, i64>(4)? as u64,
                    last_polled: row.get(5)?,
                    enabled: row.get::<_, i64>(6)? != 0,
                    created_at: row.get(7)?,
                })
            })
            .map_err(|e| BrainError::Database(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(streams)
    })
    .await
}

// =========================================================================
// RSS POLLING
// =========================================================================

/// Fetch an RSS feed, parse entries, and create nodes for new items.
/// Uses simple regex-based XML parsing to avoid heavy XML crate dependencies.
pub async fn poll_rss_stream(
    db: &Arc<BrainDb>,
    stream: &DataStream,
) -> Result<u64, BrainError> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| BrainError::Internal(format!("HTTP client error: {}", e)))?;

    let resp = client
        .get(&stream.url)
        .header("User-Agent", "ClaudeBrain/1.0")
        .send()
        .await
        .map_err(|e| BrainError::Http(e))?;

    if !resp.status().is_success() {
        return Err(BrainError::Internal(format!(
            "RSS feed returned {}",
            resp.status()
        )));
    }

    let xml = resp.text().await.unwrap_or_default();
    let items = parse_rss_items(&xml);

    let mut created = 0u64;
    let stream_id = stream.id.clone();

    for (title, description, link) in items {
        // Content-hash to deduplicate
        let hash_input = format!("{}:{}", stream.url, title);
        let content_hash = format!("{:x}", Sha256::digest(hash_input.as_bytes()));

        // Check if we already have this event
        let hash_clone = content_hash.clone();
        let already_exists = db
            .with_conn(move |conn| {
                let count: u64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM stream_events WHERE content_hash = ?1",
                        params![hash_clone],
                        |row| row.get(0),
                    )
                    .unwrap_or(0);
                Ok(count > 0)
            })
            .await?;

        if already_exists {
            continue;
        }

        // Create a stream event
        let event_id = format!("sevt:{}", uuid::Uuid::now_v7());
        let now = chrono::Utc::now().to_rfc3339();

        let eid = event_id.clone();
        let sid = stream_id.clone();
        let t = title.clone();
        let d = description.clone();
        let l = link.clone();
        let ch = content_hash.clone();
        let en = now.clone();
        db.with_conn(move |conn| {
            conn.execute(
                "INSERT INTO stream_events (id, stream_id, title, content, source_url, content_hash, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![eid, sid, t, d, l, ch, en],
            )
            .map_err(|e| BrainError::Database(e.to_string()))?;
            Ok(())
        })
        .await?;

        // Create a knowledge node
        let node_content = if description.is_empty() {
            title.clone()
        } else {
            format!("{}\n\n{}", title, description)
        };

        db.create_node(CreateNodeInput {
            title: crate::truncate_str(&title, 200).to_string(),
            content: node_content,
            domain: "news".to_string(),
            topic: stream.name.clone(),
            tags: vec!["rss".to_string(), stream.name.clone()],
            node_type: "reference".to_string(),
            source_type: "data_stream".to_string(),
            source_url: if link.is_empty() { None } else { Some(link) },
        })
        .await?;

        created += 1;
    }

    // Update last_polled timestamp
    let sid = stream.id.clone();
    let now = chrono::Utc::now().to_rfc3339();
    db.with_conn(move |conn| {
        conn.execute(
            "UPDATE data_streams SET last_polled = ?1 WHERE id = ?2",
            params![now, sid],
        )
        .map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(())
    })
    .await?;

    log::info!(
        "RSS poll '{}': {} new items from {}",
        stream.name,
        created,
        stream.url
    );
    Ok(created)
}

/// Simple regex-free XML parser for RSS items. Extracts title, description, link.
fn parse_rss_items(xml: &str) -> Vec<(String, String, String)> {
    let mut items = Vec::new();

    // Split on <item> or <entry> tags (RSS 2.0 and Atom)
    let item_splits: Vec<&str> = if xml.contains("<item>") || xml.contains("<item ") {
        xml.split("<item").skip(1).collect()
    } else if xml.contains("<entry>") || xml.contains("<entry ") {
        xml.split("<entry").skip(1).collect()
    } else {
        return items;
    };

    for item_raw in item_splits {
        let title = extract_xml_tag(item_raw, "title");
        let description = extract_xml_tag(item_raw, "description")
            .or_else(|| extract_xml_tag(item_raw, "summary"))
            .or_else(|| extract_xml_tag(item_raw, "content"))
            .unwrap_or_default();
        let link = extract_xml_tag(item_raw, "link")
            .or_else(|| extract_xml_attr(item_raw, "link", "href"))
            .unwrap_or_default();

        if let Some(t) = title {
            // Strip HTML tags from description
            let clean_desc = strip_html(&description);
            items.push((t, clean_desc, link));
        }
    }

    items
}

/// Extract text content between `<tag>` and `</tag>`.
fn extract_xml_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{}", tag);
    let close = format!("</{}>", tag);
    if let Some(start_pos) = xml.find(&open) {
        // Find the end of the opening tag (after attributes)
        let after_open = &xml[start_pos + open.len()..];
        let content_start = after_open.find('>')?;
        let content = &after_open[content_start + 1..];
        if let Some(end_pos) = content.find(&close) {
            let text = &content[..end_pos];
            // Handle CDATA
            let text = if text.starts_with("<![CDATA[") && text.ends_with("]]>") {
                &text[9..text.len() - 3]
            } else {
                text
            };
            return Some(text.trim().to_string());
        }
    }
    None
}

/// Extract an attribute value from a self-closing or opening tag.
fn extract_xml_attr(xml: &str, tag: &str, attr: &str) -> Option<String> {
    let open = format!("<{}", tag);
    if let Some(start_pos) = xml.find(&open) {
        let tag_content = &xml[start_pos..];
        let end = tag_content.find('>')?;
        let tag_str = &tag_content[..end];
        let attr_search = format!("{}=\"", attr);
        if let Some(attr_pos) = tag_str.find(&attr_search) {
            let value_start = attr_pos + attr_search.len();
            let value_rest = &tag_str[value_start..];
            if let Some(end_quote) = value_rest.find('"') {
                return Some(value_rest[..end_quote].to_string());
            }
        }
    }
    None
}

/// Strip HTML tags from a string.
fn strip_html(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }
    // Decode common HTML entities
    result
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&#x27;", "'")
        .replace("&nbsp;", " ")
}

// =========================================================================
// API POLLING
// =========================================================================

/// Fetch a JSON API endpoint, extract data, and create nodes for new entries.
pub async fn poll_api_stream(
    db: &Arc<BrainDb>,
    stream: &DataStream,
) -> Result<u64, BrainError> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| BrainError::Internal(format!("HTTP client error: {}", e)))?;

    let resp = client
        .get(&stream.url)
        .header("User-Agent", "ClaudeBrain/1.0")
        .send()
        .await
        .map_err(|e| BrainError::Http(e))?;

    if !resp.status().is_success() {
        return Err(BrainError::Internal(format!(
            "API endpoint returned {}",
            resp.status()
        )));
    }

    let body = resp.text().await.unwrap_or_default();
    let json: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| BrainError::Serialization(e))?;

    // Try to extract items from common JSON structures
    let items: Vec<&serde_json::Value> = if let Some(arr) = json.as_array() {
        arr.iter().collect()
    } else if let Some(arr) = json.get("data").and_then(|v| v.as_array()) {
        arr.iter().collect()
    } else if let Some(arr) = json.get("items").and_then(|v| v.as_array()) {
        arr.iter().collect()
    } else if let Some(arr) = json.get("results").and_then(|v| v.as_array()) {
        arr.iter().collect()
    } else {
        // Single object — treat as one item
        vec![&json]
    };

    let mut created = 0u64;
    let stream_id = stream.id.clone();

    for item in items.iter().take(50) {
        // Try to extract title and content from common fields
        let title = item
            .get("title")
            .or_else(|| item.get("name"))
            .or_else(|| item.get("headline"))
            .and_then(|v| v.as_str())
            .unwrap_or("Untitled")
            .to_string();

        let content = item
            .get("content")
            .or_else(|| item.get("description"))
            .or_else(|| item.get("body"))
            .or_else(|| item.get("text"))
            .or_else(|| item.get("summary"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| serde_json::to_string_pretty(item).unwrap_or_default());

        let url = item
            .get("url")
            .or_else(|| item.get("link"))
            .or_else(|| item.get("href"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // Content hash for dedup
        let hash_input = format!("{}:{}:{}", stream.url, title, crate::truncate_str(&content, 200));
        let content_hash = format!("{:x}", Sha256::digest(hash_input.as_bytes()));

        let hash_clone = content_hash.clone();
        let already_exists = db
            .with_conn(move |conn| {
                let count: u64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM stream_events WHERE content_hash = ?1",
                        params![hash_clone],
                        |row| row.get(0),
                    )
                    .unwrap_or(0);
                Ok(count > 0)
            })
            .await?;

        if already_exists {
            continue;
        }

        // Store stream event
        let event_id = format!("sevt:{}", uuid::Uuid::now_v7());
        let now = chrono::Utc::now().to_rfc3339();

        let eid = event_id.clone();
        let sid = stream_id.clone();
        let t = title.clone();
        let c = content.clone();
        let u = if url.is_empty() {
            None
        } else {
            Some(url.clone())
        };
        let ch = content_hash.clone();
        let en = now.clone();
        db.with_conn(move |conn| {
            conn.execute(
                "INSERT INTO stream_events (id, stream_id, title, content, source_url, content_hash, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![eid, sid, t, c, u, ch, en],
            )
            .map_err(|e| BrainError::Database(e.to_string()))?;
            Ok(())
        })
        .await?;

        // Create knowledge node
        db.create_node(CreateNodeInput {
            title: crate::truncate_str(&title, 200).to_string(),
            content,
            domain: "data".to_string(),
            topic: stream.name.clone(),
            tags: vec!["api_stream".to_string(), stream.name.clone()],
            node_type: "reference".to_string(),
            source_type: "data_stream".to_string(),
            source_url: if url.is_empty() {
                Some(stream.url.clone())
            } else {
                Some(url)
            },
        })
        .await?;

        created += 1;
    }

    // Update last_polled
    let sid = stream.id.clone();
    let now = chrono::Utc::now().to_rfc3339();
    db.with_conn(move |conn| {
        conn.execute(
            "UPDATE data_streams SET last_polled = ?1 WHERE id = ?2",
            params![now, sid],
        )
        .map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(())
    })
    .await?;

    log::info!(
        "API poll '{}': {} new items from {}",
        stream.name,
        created,
        stream.url
    );
    Ok(created)
}

// =========================================================================
// STREAM POLLER (background task)
// =========================================================================

/// Poll all enabled streams that are due for a check.
pub async fn poll_all_streams(db: &Arc<BrainDb>) -> Result<u64, BrainError> {
    let streams = get_streams(db).await?;
    let now = chrono::Utc::now();
    let mut total_new = 0u64;

    for stream in &streams {
        if !stream.enabled {
            continue;
        }

        // Check if enough time has passed since last poll
        let should_poll = match &stream.last_polled {
            None => true,
            Some(last) => {
                if let Ok(last_time) = chrono::DateTime::parse_from_rfc3339(last) {
                    let elapsed = now.signed_duration_since(last_time);
                    elapsed.num_minutes() >= stream.poll_interval_mins as i64
                } else {
                    true
                }
            }
        };

        if !should_poll {
            continue;
        }

        let result = match stream.stream_type.as_str() {
            "rss" => poll_rss_stream(db, stream).await,
            "api_poll" => poll_api_stream(db, stream).await,
            "file_watch" => {
                // File watch is handled by the existing notify watcher
                log::debug!("file_watch stream '{}' — skipping (handled by notify)", stream.name);
                Ok(0)
            }
            "webhook" => {
                // Webhooks are push-based, nothing to poll
                log::debug!("webhook stream '{}' — skipping (push-based)", stream.name);
                Ok(0)
            }
            other => {
                log::warn!("Unknown stream type '{}' for stream '{}'", other, stream.name);
                Ok(0)
            }
        };

        match result {
            Ok(count) => total_new += count,
            Err(e) => {
                log::warn!("Stream poll error for '{}': {}", stream.name, e);
            }
        }
    }

    Ok(total_new)
}

/// Background loop that periodically polls all data streams.
/// Runs every 5 minutes, checking each stream against its own interval.
pub async fn run_stream_poller(db: Arc<BrainDb>) {
    log::info!("Data stream poller started");

    // Initial delay to let the brain fully initialize
    tokio::time::sleep(std::time::Duration::from_secs(60)).await;

    loop {
        match poll_all_streams(&db).await {
            Ok(count) => {
                if count > 0 {
                    log::info!("Stream poller: {} new items ingested", count);
                }
            }
            Err(e) => {
                log::warn!("Stream poller error: {}", e);
            }
        }

        // Check every 5 minutes
        tokio::time::sleep(std::time::Duration::from_secs(300)).await;
    }
}
