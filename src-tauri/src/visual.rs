//! Phase Omega Part V — Visual Intelligence
//!
//! Screenshot and image analysis using Ollama vision models (LLaVA / Moondream).
//! Images are base64-encoded and sent to Ollama's `/api/generate` endpoint
//! with image support. The analysis extracts descriptions, entities, and
//! context, storing results as both a `visual_analysis` record and a
//! knowledge node in the brain graph.
//!
//! ## Functions
//!
//! - `analyze_image` — read an image file, send to vision LLM, store analysis
//! - `analyze_screenshot` — analyze a screenshot from a given file path
//! - `ingest_diagram` — specialized analysis for architecture/system diagrams

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
pub struct VisualAnalysis {
    pub id: String,
    pub image_path: String,
    pub description: String,
    pub entities: Vec<String>,
    pub context: String,
    pub node_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
struct OllamaVisionResponse {
    response: String,
}

// =========================================================================
// BASE64 ENCODER (no crate dependency)
// =========================================================================

fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity((data.len() + 2) / 3 * 4);
    let chunks = data.chunks(3);
    for chunk in chunks {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

// =========================================================================
// VISION ANALYSIS VIA OLLAMA
// =========================================================================

/// Send an image to Ollama's vision model and get a structured analysis.
async fn call_vision_model(
    ollama_url: &str,
    image_b64: &str,
    prompt: &str,
) -> Result<String, BrainError> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| BrainError::Internal(format!("HTTP client error: {}", e)))?;

    let body = serde_json::json!({
        "model": "moondream",
        "prompt": prompt,
        "images": [image_b64],
        "stream": false
    });

    let resp = client
        .post(format!("{}/api/generate", ollama_url))
        .json(&body)
        .send()
        .await
        .map_err(|e| BrainError::Embedding(format!("Vision model request failed: {}", e)))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(BrainError::Embedding(format!(
            "Vision model returned {}: {}. Make sure 'moondream' model is installed via `ollama pull moondream`",
            status, text
        )));
    }

    let data: OllamaVisionResponse = resp
        .json()
        .await
        .map_err(|e| BrainError::Embedding(format!("Failed to parse vision response: {}", e)))?;

    Ok(data.response)
}

/// Parse LLM output into description, entities, and context.
fn parse_analysis(raw: &str) -> (String, Vec<String>, String) {
    let mut description = String::new();
    let mut entities: Vec<String> = Vec::new();
    let mut context = String::new();

    let mut section = "description";
    for line in raw.lines() {
        let trimmed = line.trim();
        let lower = trimmed.to_lowercase();
        if lower.starts_with("entities:") || lower.starts_with("## entities") {
            section = "entities";
            let after = trimmed.splitn(2, ':').nth(1).unwrap_or("").trim();
            if !after.is_empty() {
                for e in after.split(',') {
                    let e = e.trim().trim_start_matches('-').trim();
                    if !e.is_empty() {
                        entities.push(e.to_string());
                    }
                }
            }
            continue;
        }
        if lower.starts_with("context:") || lower.starts_with("## context") {
            section = "context";
            let after = trimmed.splitn(2, ':').nth(1).unwrap_or("").trim();
            if !after.is_empty() {
                context.push_str(after);
            }
            continue;
        }
        if lower.starts_with("description:") || lower.starts_with("## description") {
            section = "description";
            let after = trimmed.splitn(2, ':').nth(1).unwrap_or("").trim();
            if !after.is_empty() {
                description.push_str(after);
                description.push(' ');
            }
            continue;
        }

        match section {
            "description" => {
                if !trimmed.is_empty() {
                    if !description.is_empty() {
                        description.push(' ');
                    }
                    description.push_str(trimmed);
                }
            }
            "entities" => {
                let cleaned = trimmed.trim_start_matches('-').trim_start_matches('*').trim();
                if !cleaned.is_empty() {
                    for e in cleaned.split(',') {
                        let e = e.trim();
                        if !e.is_empty() {
                            entities.push(e.to_string());
                        }
                    }
                }
            }
            "context" => {
                if !trimmed.is_empty() {
                    if !context.is_empty() {
                        context.push(' ');
                    }
                    context.push_str(trimmed);
                }
            }
            _ => {}
        }
    }

    // Fallback: if the LLM didn't use sections, treat the whole response as description
    if description.is_empty() && entities.is_empty() && context.is_empty() {
        description = raw.trim().to_string();
    }

    (description, entities, context)
}

// =========================================================================
// PUBLIC API
// =========================================================================

/// Analyze an image file using the vision model. Creates a node and stores
/// the analysis in the `visual_analysis` table.
pub async fn analyze_image(
    db: &Arc<BrainDb>,
    image_path: &str,
) -> Result<VisualAnalysis, BrainError> {
    let path = std::path::Path::new(image_path);
    if !path.exists() {
        return Err(BrainError::NotFound(format!(
            "Image file not found: {}",
            image_path
        )));
    }

    let image_data = std::fs::read(path)?;
    let image_b64 = base64_encode(&image_data);

    let prompt = "Analyze this image in detail. Respond in this exact format:\n\
                  Description: <detailed description of what's in the image>\n\
                  Entities: <comma-separated list of key entities, objects, text, or concepts visible>\n\
                  Context: <what this image is about, its purpose, or what scenario it depicts>";

    let ollama_url = db.config.ollama_url.clone();
    let raw_response = call_vision_model(&ollama_url, &image_b64, prompt).await?;
    let (description, entities, context) = parse_analysis(&raw_response);

    let filename = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("image");
    let title = format!("Visual: {}", filename);

    // Create a knowledge node for this analysis
    let node_content = format!(
        "Image analysis of '{}'\n\nDescription: {}\n\nEntities: {}\n\nContext: {}",
        image_path,
        description,
        entities.join(", "),
        context
    );

    let node = db
        .create_node(CreateNodeInput {
            title: title.clone(),
            content: node_content,
            domain: "visual".to_string(),
            topic: "image_analysis".to_string(),
            tags: entities.clone(),
            node_type: "reference".to_string(),
            source_type: "visual_analysis".to_string(),
            source_url: Some(image_path.to_string()),
        })
        .await?;

    // Store in the visual_analysis table
    let analysis_id = format!("va:{}", uuid::Uuid::now_v7());
    let now = chrono::Utc::now().to_rfc3339();
    let entities_json = serde_json::to_string(&entities).unwrap_or_else(|_| "[]".to_string());
    let node_id = node.id.clone();

    let va = VisualAnalysis {
        id: analysis_id.clone(),
        image_path: image_path.to_string(),
        description: description.clone(),
        entities: entities.clone(),
        context: context.clone(),
        node_id: Some(node_id.clone()),
        created_at: now.clone(),
    };

    let image_path_owned = image_path.to_string();
    db.with_conn(move |conn| {
        conn.execute(
            "INSERT INTO visual_analysis (id, image_path, description, entities, context, node_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![analysis_id, image_path_owned, description, entities_json, context, node_id, now],
        )
        .map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(())
    })
    .await?;

    log::info!("Visual analysis complete for '{}' -> node {}", va.image_path, va.node_id.as_deref().unwrap_or("none"));
    Ok(va)
}

/// Analyze a screenshot from a file path. Delegates to `analyze_image` with
/// a screenshot-optimized prompt.
pub async fn analyze_screenshot(
    db: &Arc<BrainDb>,
    screenshot_path: &str,
) -> Result<VisualAnalysis, BrainError> {
    let path = std::path::Path::new(screenshot_path);
    if !path.exists() {
        return Err(BrainError::NotFound(format!(
            "Screenshot file not found: {}",
            screenshot_path
        )));
    }

    let image_data = std::fs::read(path)?;
    let image_b64 = base64_encode(&image_data);

    let prompt = "This is a screenshot. Analyze it in detail. Respond in this exact format:\n\
                  Description: <what application or content is shown, what the user is doing>\n\
                  Entities: <comma-separated list of visible UI elements, text, application names, file names, etc>\n\
                  Context: <what task or workflow this screenshot relates to>";

    let ollama_url = db.config.ollama_url.clone();
    let raw_response = call_vision_model(&ollama_url, &image_b64, prompt).await?;
    let (description, entities, context) = parse_analysis(&raw_response);

    let filename = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("screenshot");
    let title = format!("Screenshot: {}", filename);

    let node_content = format!(
        "Screenshot analysis of '{}'\n\nDescription: {}\n\nEntities: {}\n\nContext: {}",
        screenshot_path,
        description,
        entities.join(", "),
        context
    );

    let node = db
        .create_node(CreateNodeInput {
            title,
            content: node_content,
            domain: "visual".to_string(),
            topic: "screenshot".to_string(),
            tags: entities.clone(),
            node_type: "reference".to_string(),
            source_type: "visual_analysis".to_string(),
            source_url: Some(screenshot_path.to_string()),
        })
        .await?;

    let analysis_id = format!("va:{}", uuid::Uuid::now_v7());
    let now = chrono::Utc::now().to_rfc3339();
    let entities_json = serde_json::to_string(&entities).unwrap_or_else(|_| "[]".to_string());
    let node_id = node.id.clone();

    let va = VisualAnalysis {
        id: analysis_id.clone(),
        image_path: screenshot_path.to_string(),
        description: description.clone(),
        entities: entities.clone(),
        context: context.clone(),
        node_id: Some(node_id.clone()),
        created_at: now.clone(),
    };

    let screenshot_path_owned = screenshot_path.to_string();
    db.with_conn(move |conn| {
        conn.execute(
            "INSERT INTO visual_analysis (id, image_path, description, entities, context, node_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![analysis_id, screenshot_path_owned, description, entities_json, context, node_id, now],
        )
        .map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(())
    })
    .await?;

    log::info!("Screenshot analysis complete for '{}'", va.image_path);
    Ok(va)
}

/// Specialized analysis for architecture diagrams. Extracts entities and
/// relationships, creating nodes and edges in the brain graph.
pub async fn ingest_diagram(
    db: &Arc<BrainDb>,
    image_path: &str,
) -> Result<VisualAnalysis, BrainError> {
    let path = std::path::Path::new(image_path);
    if !path.exists() {
        return Err(BrainError::NotFound(format!(
            "Diagram file not found: {}",
            image_path
        )));
    }

    let image_data = std::fs::read(path)?;
    let image_b64 = base64_encode(&image_data);

    let prompt = "This is an architecture or system diagram. Analyze it thoroughly. Respond in this exact format:\n\
                  Description: <overall description of the architecture/system shown>\n\
                  Entities: <comma-separated list of all components, services, databases, APIs, and systems visible>\n\
                  Context: <the relationships between components — what connects to what, data flow directions, dependencies>\n\n\
                  Be as specific as possible about component names and their connections.";

    let ollama_url = db.config.ollama_url.clone();
    let raw_response = call_vision_model(&ollama_url, &image_b64, prompt).await?;
    let (description, entities, context) = parse_analysis(&raw_response);

    let filename = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("diagram");
    let title = format!("Diagram: {}", filename);

    let node_content = format!(
        "Architecture diagram analysis of '{}'\n\nDescription: {}\n\nComponents: {}\n\nRelationships: {}",
        image_path,
        description,
        entities.join(", "),
        context
    );

    // Create main diagram node
    let main_node = db
        .create_node(CreateNodeInput {
            title,
            content: node_content,
            domain: "architecture".to_string(),
            topic: "diagram".to_string(),
            tags: entities.clone(),
            node_type: "reference".to_string(),
            source_type: "visual_analysis".to_string(),
            source_url: Some(image_path.to_string()),
        })
        .await?;

    // Create child nodes for each entity and link them to the main node
    let main_node_id = main_node.id.clone();
    for entity in &entities {
        let entity_node = db
            .create_node(CreateNodeInput {
                title: entity.clone(),
                content: format!("Component '{}' identified in diagram '{}'", entity, filename),
                domain: "architecture".to_string(),
                topic: "component".to_string(),
                tags: vec!["diagram_entity".to_string()],
                node_type: "concept".to_string(),
                source_type: "visual_analysis".to_string(),
                source_url: Some(image_path.to_string()),
            })
            .await?;

        // Link entity to diagram
        let edge_id = format!("edge:{}", uuid::Uuid::now_v7());
        let now_edge = chrono::Utc::now().to_rfc3339();
        let eid = entity_node.id.clone();
        let mid = main_node_id.clone();
        db.with_conn(move |conn| {
            conn.execute(
                "INSERT OR IGNORE INTO edges (id, source_id, target_id, relation_type, strength, discovered_by, evidence, animated, created_at)
                 VALUES (?1, ?2, ?3, 'part_of', 0.8, 'visual_analysis', 'Extracted from diagram', 1, ?4)",
                params![edge_id, eid, mid, now_edge],
            )
            .map_err(|e| BrainError::Database(e.to_string()))?;
            Ok(())
        })
        .await?;
    }

    // Store the visual_analysis record
    let analysis_id = format!("va:{}", uuid::Uuid::now_v7());
    let now = chrono::Utc::now().to_rfc3339();
    let entities_json = serde_json::to_string(&entities).unwrap_or_else(|_| "[]".to_string());
    let node_id = main_node.id.clone();

    let va = VisualAnalysis {
        id: analysis_id.clone(),
        image_path: image_path.to_string(),
        description: description.clone(),
        entities: entities.clone(),
        context: context.clone(),
        node_id: Some(node_id.clone()),
        created_at: now.clone(),
    };

    let image_path_owned = image_path.to_string();
    let entity_count = entities.len();
    db.with_conn(move |conn| {
        conn.execute(
            "INSERT INTO visual_analysis (id, image_path, description, entities, context, node_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![analysis_id, image_path_owned, description, entities_json, context, node_id, now],
        )
        .map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(())
    })
    .await?;

    log::info!(
        "Diagram ingestion complete for '{}': {} entities extracted",
        va.image_path,
        entity_count
    );
    Ok(va)
}
