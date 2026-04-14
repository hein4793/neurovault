//! Phase Omega Part V — Audio Intelligence
//!
//! Audio transcription pipeline that connects to a local Whisper endpoint
//! (whisper.cpp server at `http://localhost:8766/transcribe`) for speech-to-text.
//! After transcription, uses the brain's LLM to extract actionable insights:
//! action items, key decisions, and structured information.
//!
//! ## Functions
//!
//! - `transcribe_audio` — send audio file to Whisper endpoint, get text back
//! - `extract_meeting_insights` — LLM-driven extraction of action items and decisions
//! - `ingest_voice_note` — transcribe + extract + create nodes (full pipeline)

use crate::commands::ai::get_llm_client_deep;
use crate::db::models::CreateNodeInput;
use crate::db::BrainDb;
use crate::error::BrainError;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// =========================================================================
// DATA STRUCTURES
// =========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transcription {
    pub id: String,
    pub audio_path: String,
    pub text: String,
    pub duration_seconds: u64,
    pub action_items: Vec<String>,
    pub key_decisions: Vec<String>,
    pub node_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingInsights {
    pub action_items: Vec<String>,
    pub key_decisions: Vec<String>,
    pub key_information: Vec<String>,
    pub summary: String,
}

#[derive(Debug, Deserialize)]
struct WhisperResponse {
    #[serde(default)]
    text: String,
    #[serde(default)]
    duration: Option<f64>,
}

// =========================================================================
// WHISPER TRANSCRIPTION
// =========================================================================

const DEFAULT_WHISPER_URL: &str = "http://localhost:8766";

/// Transcribe an audio file by sending it to a local Whisper endpoint.
/// If the endpoint is not available, returns an error with installation
/// instructions.
pub async fn transcribe_audio(
    db: &Arc<BrainDb>,
    audio_path: &str,
) -> Result<Transcription, BrainError> {
    let path = std::path::Path::new(audio_path);
    if !path.exists() {
        return Err(BrainError::NotFound(format!(
            "Audio file not found: {}",
            audio_path
        )));
    }

    // Read audio file
    let audio_data = std::fs::read(path)?;
    let filename = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("audio.wav")
        .to_string();

    // Determine the whisper URL from settings or use default
    let whisper_url = get_whisper_url(db);

    // Check if Whisper endpoint is available
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| BrainError::Internal(format!("HTTP client error: {}", e)))?;

    // Try health check first
    let health_check = client
        .get(format!("{}/health", whisper_url))
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await;

    if health_check.is_err() {
        return Err(BrainError::Internal(format!(
            "Whisper transcription service not available at {}. \
             Please install and start whisper.cpp server:\n\
             1. Clone: git clone https://github.com/ggerganov/whisper.cpp\n\
             2. Build: make server\n\
             3. Run: ./server -m models/ggml-base.en.bin --port 8766\n\
             Or configure a custom endpoint in brain settings.",
            whisper_url
        )));
    }

    // Build a minimal multipart/form-data body manually (no reqwest
    // multipart feature needed). The whisper.cpp server expects a
    // multipart form with a "file" field containing the audio bytes.
    let boundary = format!("----ClaudeBrain{}", uuid::Uuid::now_v7());
    let mut body_bytes: Vec<u8> = Vec::new();
    body_bytes.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
    body_bytes.extend_from_slice(
        format!(
            "Content-Disposition: form-data; name=\"file\"; filename=\"{}\"\r\n",
            filename
        )
        .as_bytes(),
    );
    body_bytes.extend_from_slice(b"Content-Type: application/octet-stream\r\n\r\n");
    body_bytes.extend_from_slice(&audio_data);
    body_bytes.extend_from_slice(format!("\r\n--{}--\r\n", boundary).as_bytes());

    let resp = client
        .post(format!("{}/inference", whisper_url))
        .header(
            "Content-Type",
            format!("multipart/form-data; boundary={}", boundary),
        )
        .body(body_bytes)
        .send()
        .await
        .map_err(|e| {
            BrainError::Embedding(format!(
                "Whisper transcription request failed: {}",
                e
            ))
        })?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();
        return Err(BrainError::Embedding(format!(
            "Whisper returned {}: {}",
            status, body_text
        )));
    }

    let text = resp.text().await.unwrap_or_default();

    // Try to parse as JSON first, fall back to raw text
    let (transcript_text, duration_secs) = if let Ok(parsed) =
        serde_json::from_str::<WhisperResponse>(&text)
    {
        (
            parsed.text.trim().to_string(),
            parsed.duration.map(|d| d as u64).unwrap_or(0),
        )
    } else {
        (text.trim().to_string(), 0u64)
    };

    if transcript_text.is_empty() {
        return Err(BrainError::Internal(
            "Whisper returned empty transcription".to_string(),
        ));
    }

    // Store in the transcriptions table
    let transcription_id = format!("tr:{}", uuid::Uuid::now_v7());
    let now = chrono::Utc::now().to_rfc3339();

    let t = Transcription {
        id: transcription_id.clone(),
        audio_path: audio_path.to_string(),
        text: transcript_text.clone(),
        duration_seconds: duration_secs,
        action_items: Vec::new(),
        key_decisions: Vec::new(),
        node_id: None,
        created_at: now.clone(),
    };

    let ap = audio_path.to_string();
    let tt = transcript_text.clone();
    let tid = transcription_id.clone();
    let tnow = now.clone();
    db.with_conn(move |conn| {
        conn.execute(
            "INSERT INTO transcriptions (id, audio_path, text, duration_seconds, action_items, key_decisions, node_id, created_at)
             VALUES (?1, ?2, ?3, ?4, '[]', '[]', NULL, ?5)",
            params![tid, ap, tt, duration_secs, tnow],
        )
        .map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(())
    })
    .await?;

    log::info!(
        "Transcription complete for '{}': {} chars, {}s",
        audio_path,
        transcript_text.len(),
        duration_secs
    );
    Ok(t)
}

// =========================================================================
// MEETING INSIGHTS EXTRACTION
// =========================================================================

/// Use the brain's LLM to extract action items, decisions, and key
/// information from a transcription text.
pub async fn extract_meeting_insights(
    db: &Arc<BrainDb>,
    transcription: &str,
) -> Result<MeetingInsights, BrainError> {
    let llm = get_llm_client_deep(db);

    let prompt = format!(
        "Analyze the following transcription and extract structured insights. \
         Respond in this exact format — use ONLY these headers, one item per line under each:\n\n\
         SUMMARY:\n<1-3 sentence summary of the content>\n\n\
         ACTION_ITEMS:\n- <action item 1>\n- <action item 2>\n\n\
         KEY_DECISIONS:\n- <decision 1>\n- <decision 2>\n\n\
         KEY_INFORMATION:\n- <important fact 1>\n- <important fact 2>\n\n\
         If a section has no items, write 'None' under it.\n\n\
         TRANSCRIPTION:\n{}",
        crate::truncate_str(transcription, 6000)
    );

    let response = llm.generate(&prompt, 2000).await?;
    let insights = parse_meeting_insights(&response);
    Ok(insights)
}

/// Parse the LLM's structured output into MeetingInsights.
fn parse_meeting_insights(raw: &str) -> MeetingInsights {
    let mut summary = String::new();
    let mut action_items: Vec<String> = Vec::new();
    let mut key_decisions: Vec<String> = Vec::new();
    let mut key_information: Vec<String> = Vec::new();
    let mut section = "";

    for line in raw.lines() {
        let trimmed = line.trim();
        let upper = trimmed.to_uppercase();

        if upper.starts_with("SUMMARY:") {
            section = "summary";
            let after = trimmed.splitn(2, ':').nth(1).unwrap_or("").trim();
            if !after.is_empty() {
                summary.push_str(after);
            }
            continue;
        }
        if upper.starts_with("ACTION_ITEMS:") || upper.starts_with("ACTION ITEMS:") {
            section = "action_items";
            continue;
        }
        if upper.starts_with("KEY_DECISIONS:") || upper.starts_with("KEY DECISIONS:") {
            section = "key_decisions";
            continue;
        }
        if upper.starts_with("KEY_INFORMATION:") || upper.starts_with("KEY INFORMATION:") {
            section = "key_information";
            continue;
        }

        let item = trimmed
            .trim_start_matches('-')
            .trim_start_matches('*')
            .trim_start_matches("• ")
            .trim();
        if item.is_empty() || item.to_lowercase() == "none" {
            continue;
        }

        match section {
            "summary" => {
                if !summary.is_empty() {
                    summary.push(' ');
                }
                summary.push_str(item);
            }
            "action_items" => action_items.push(item.to_string()),
            "key_decisions" => key_decisions.push(item.to_string()),
            "key_information" => key_information.push(item.to_string()),
            _ => {}
        }
    }

    MeetingInsights {
        action_items,
        key_decisions,
        key_information,
        summary,
    }
}

/// Full voice note pipeline: transcribe -> extract insights -> create nodes.
pub async fn ingest_voice_note(
    db: &Arc<BrainDb>,
    audio_path: &str,
) -> Result<Transcription, BrainError> {
    // Step 1: Transcribe
    let mut transcription = transcribe_audio(db, audio_path).await?;

    // Step 2: Extract insights
    let insights = extract_meeting_insights(db, &transcription.text).await?;
    transcription.action_items = insights.action_items.clone();
    transcription.key_decisions = insights.key_decisions.clone();

    // Step 3: Create main transcription node
    let filename = std::path::Path::new(audio_path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("voice_note");

    let node_content = format!(
        "Voice note transcription: '{}'\n\nSummary: {}\n\nFull text: {}\n\nAction Items:\n{}\n\nKey Decisions:\n{}",
        filename,
        insights.summary,
        crate::truncate_str(&transcription.text, 4000),
        insights.action_items.iter().map(|a| format!("- {}", a)).collect::<Vec<_>>().join("\n"),
        insights.key_decisions.iter().map(|d| format!("- {}", d)).collect::<Vec<_>>().join("\n"),
    );

    let mut tags = vec!["voice_note".to_string(), "transcription".to_string()];
    // Add action items and decisions as tags (truncated)
    for ai in &insights.action_items {
        let short = crate::truncate_str(ai, 50);
        tags.push(format!("action:{}", short));
    }

    let node = db
        .create_node(CreateNodeInput {
            title: format!("Voice Note: {}", filename),
            content: node_content,
            domain: "audio".to_string(),
            topic: "voice_note".to_string(),
            tags,
            node_type: "reference".to_string(),
            source_type: "transcription".to_string(),
            source_url: Some(audio_path.to_string()),
        })
        .await?;

    transcription.node_id = Some(node.id.clone());

    // Step 4: Create individual action item nodes and link them
    let main_node_id = node.id.clone();
    for action in &insights.action_items {
        let action_node = db
            .create_node(CreateNodeInput {
                title: format!("Action: {}", crate::truncate_str(action, 80)),
                content: format!(
                    "Action item from voice note '{}': {}",
                    filename, action
                ),
                domain: "tasks".to_string(),
                topic: "action_item".to_string(),
                tags: vec!["action_item".to_string(), "from_voice".to_string()],
                node_type: "decision".to_string(),
                source_type: "transcription".to_string(),
                source_url: Some(audio_path.to_string()),
            })
            .await?;

        let edge_id = format!("edge:{}", uuid::Uuid::now_v7());
        let now_edge = chrono::Utc::now().to_rfc3339();
        let aid = action_node.id.clone();
        let mid = main_node_id.clone();
        db.with_conn(move |conn| {
            conn.execute(
                "INSERT OR IGNORE INTO edges (id, source_id, target_id, relation_type, strength, discovered_by, evidence, animated, created_at)
                 VALUES (?1, ?2, ?3, 'derived_from', 0.9, 'audio_intelligence', 'Extracted from voice note transcription', 1, ?4)",
                params![edge_id, aid, mid, now_edge],
            )
            .map_err(|e| BrainError::Database(e.to_string()))?;
            Ok(())
        })
        .await?;
    }

    // Step 5: Update the transcription record with insights and node_id
    let tid = transcription.id.clone();
    let ai_json =
        serde_json::to_string(&transcription.action_items).unwrap_or_else(|_| "[]".to_string());
    let kd_json =
        serde_json::to_string(&transcription.key_decisions).unwrap_or_else(|_| "[]".to_string());
    let nid = node.id.clone();

    db.with_conn(move |conn| {
        conn.execute(
            "UPDATE transcriptions SET action_items = ?1, key_decisions = ?2, node_id = ?3 WHERE id = ?4",
            params![ai_json, kd_json, nid, tid],
        )
        .map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(())
    })
    .await?;

    log::info!(
        "Voice note ingestion complete for '{}': {} action items, {} decisions",
        audio_path,
        insights.action_items.len(),
        insights.key_decisions.len()
    );
    Ok(transcription)
}

// =========================================================================
// HELPERS
// =========================================================================

fn get_whisper_url(db: &BrainDb) -> String {
    let settings_path = db.config.data_dir.join("settings.json");
    if settings_path.exists() {
        if let Ok(data) = std::fs::read_to_string(&settings_path) {
            if let Ok(s) = serde_json::from_str::<serde_json::Value>(&data) {
                if let Some(url) = s.get("whisper_url").and_then(|v| v.as_str()) {
                    return url.to_string();
                }
            }
        }
    }
    DEFAULT_WHISPER_URL.to_string()
}
