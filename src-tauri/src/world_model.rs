//! Phase Omega Part III — Causal World Model
//!
//! Builds and maintains a causal model of the world based on brain knowledge.
//! Entities (competitors, markets, products, customer segments) are extracted
//! from brain nodes and connected by causal links (increases, decreases,
//! enables, blocks). The model supports forward-propagation scenario
//! simulation: given a trigger event, trace the causal chain to predict
//! cascading effects with estimated magnitudes and timeframes.
//!
//! ## Architecture
//!
//! - `WorldEntity` — a named thing in the world with typed properties
//! - `CausalLink` — a directed relationship between two entities
//! - `CausalPrediction` — the output of a scenario simulation
//! - `build_causal_model()` — LLM-driven extraction of entities & links
//! - `simulate_scenario()` — forward propagation through the causal graph

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
pub struct WorldEntity {
    pub id: String,
    pub name: String,
    pub entity_type: String, // "competitor", "market_factor", "product", "customer_segment"
    pub properties: HashMap<String, f64>,
    pub last_updated: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CausalLink {
    pub id: String,
    pub cause_id: String,
    pub effect_id: String,
    pub relationship: String, // "increases", "decreases", "enables", "blocks"
    pub strength: f32,
    pub lag_days: u32,
    pub evidence_node_ids: Vec<String>,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CausalPrediction {
    pub trigger: String,
    pub predicted_effects: Vec<PredictedEffect>,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictedEffect {
    pub entity: String,
    pub property: String,
    pub direction: String, // "increase", "decrease"
    pub magnitude: f32,
    pub timeframe_days: u32,
}

// =========================================================================
// DB HELPERS
// =========================================================================

/// Load all world entities from SQLite.
pub async fn get_all_entities(db: &Arc<BrainDb>) -> Result<Vec<WorldEntity>, BrainError> {
    db.with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT id, name, entity_type, properties, last_updated FROM world_entities",
            )
            .map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt
            .query_map([], |row| {
                let props_json: String = row.get(3)?;
                Ok(WorldEntity {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    entity_type: row.get(2)?,
                    properties: serde_json::from_str(&props_json).unwrap_or_default(),
                    last_updated: row.get(4)?,
                })
            })
            .map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| BrainError::Database(e.to_string()))?);
        }
        Ok(result)
    })
    .await
}

/// Load all causal links from SQLite.
pub async fn get_all_links(db: &Arc<BrainDb>) -> Result<Vec<CausalLink>, BrainError> {
    db.with_conn(|conn| {
        let mut stmt = conn
            .prepare(
                "SELECT id, cause_id, effect_id, relationship, strength, lag_days, \
                 evidence_node_ids, confidence FROM causal_links",
            )
            .map_err(|e| BrainError::Database(e.to_string()))?;
        let rows = stmt
            .query_map([], |row| {
                let evidence_json: String = row.get(6)?;
                Ok(CausalLink {
                    id: row.get(0)?,
                    cause_id: row.get(1)?,
                    effect_id: row.get(2)?,
                    relationship: row.get(3)?,
                    strength: row.get(4)?,
                    lag_days: row.get(5)?,
                    evidence_node_ids: serde_json::from_str(&evidence_json).unwrap_or_default(),
                    confidence: row.get(7)?,
                })
            })
            .map_err(|e| BrainError::Database(e.to_string()))?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| BrainError::Database(e.to_string()))?);
        }
        Ok(result)
    })
    .await
}

/// Insert or update a world entity.
async fn upsert_entity(db: &Arc<BrainDb>, entity: WorldEntity) -> Result<(), BrainError> {
    db.with_conn(move |conn| {
        let props_json =
            serde_json::to_string(&entity.properties).unwrap_or_else(|_| "{}".to_string());
        conn.execute(
            "INSERT INTO world_entities (id, name, entity_type, properties, last_updated) \
             VALUES (?1, ?2, ?3, ?4, ?5) \
             ON CONFLICT(id) DO UPDATE SET \
               name = excluded.name, \
               entity_type = excluded.entity_type, \
               properties = excluded.properties, \
               last_updated = excluded.last_updated",
            params![
                entity.id,
                entity.name,
                entity.entity_type,
                props_json,
                entity.last_updated
            ],
        )
        .map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(())
    })
    .await
}

/// Insert a causal link.
async fn insert_link(db: &Arc<BrainDb>, link: CausalLink) -> Result<(), BrainError> {
    db.with_conn(move |conn| {
        let evidence_json =
            serde_json::to_string(&link.evidence_node_ids).unwrap_or_else(|_| "[]".to_string());
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT OR IGNORE INTO causal_links \
             (id, cause_id, effect_id, relationship, strength, lag_days, \
              evidence_node_ids, confidence, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                link.id,
                link.cause_id,
                link.effect_id,
                link.relationship,
                link.strength,
                link.lag_days,
                evidence_json,
                link.confidence,
                now
            ],
        )
        .map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(())
    })
    .await
}

// =========================================================================
// CAUSAL MODEL BUILDER
// =========================================================================

/// Build the causal model by querying the brain for business/technology nodes,
/// then using the DEEP LLM to extract entities and causal relationships.
pub async fn build_causal_model(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    // 1. Query recent business & technology nodes
    let nodes: Vec<(String, String, String, String)> = db
        .with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT id, title, summary, domain FROM nodes \
                     WHERE domain IN ('business', 'technology') \
                       AND summary != '' \
                       AND length(summary) > 20 \
                     ORDER BY created_at DESC \
                     LIMIT 50",
                )
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                })
                .map_err(|e| BrainError::Database(e.to_string()))?;
            let mut result = Vec::new();
            for row in rows {
                result.push(row.map_err(|e| BrainError::Database(e.to_string()))?);
            }
            Ok(result)
        })
        .await?;

    if nodes.is_empty() {
        return Ok("No business/technology nodes found to build model from".to_string());
    }

    // 2. Build context from node summaries
    let mut context = String::new();
    let mut node_ids: Vec<String> = Vec::new();
    for (id, title, summary, domain) in &nodes {
        context.push_str(&format!("[{}] {} ({}): {}\n", domain, title, id, summary));
        node_ids.push(id.clone());
    }

    // 3. Use DEEP LLM to extract entities and causal relationships
    let llm = crate::commands::ai::get_llm_client_deep(db);
    let prompt = format!(
        "Analyze the following knowledge base entries and extract:\n\
         1. Key entities (competitors, market factors, products, customer segments, technologies)\n\
         2. Causal relationships between entities\n\n\
         KNOWLEDGE:\n{}\n\n\
         Respond in STRICT JSON format:\n\
         {{\n\
           \"entities\": [\n\
             {{\"name\": \"...\", \"type\": \"competitor|market_factor|product|customer_segment|technology\", \"properties\": {{\"relevance\": 0.8}}}}\n\
           ],\n\
           \"relationships\": [\n\
             {{\"cause\": \"entity_name\", \"effect\": \"entity_name\", \"relationship\": \"increases|decreases|enables|blocks\", \"strength\": 0.7, \"lag_days\": 30, \"confidence\": 0.6}}\n\
           ]\n\
         }}\n\n\
         Extract at most 15 entities and 20 relationships. Only include clear causal relationships, not mere correlations.",
        context
    );

    let response = llm.generate(&prompt, 2000).await.map_err(|e| {
        BrainError::Internal(format!("LLM failed during causal model extraction: {}", e))
    })?;

    // 4. Parse the LLM response
    let parsed = parse_causal_extraction(&response);
    let now = chrono::Utc::now().to_rfc3339();

    let mut entities_created = 0u32;
    let mut links_created = 0u32;

    // Build a name→id map for linking
    let mut name_to_id: HashMap<String, String> = HashMap::new();

    // 5. Store entities
    for extracted in &parsed.entities {
        let id = format!("world_entity:{}", uuid::Uuid::now_v7());
        name_to_id.insert(extracted.name.to_lowercase(), id.clone());
        let entity = WorldEntity {
            id,
            name: extracted.name.clone(),
            entity_type: extracted.entity_type.clone(),
            properties: extracted.properties.clone(),
            last_updated: now.clone(),
        };
        if upsert_entity(db, entity).await.is_ok() {
            entities_created += 1;
        }
    }

    // 6. Store causal links
    for rel in &parsed.relationships {
        let cause_key = rel.cause.to_lowercase();
        let effect_key = rel.effect.to_lowercase();
        if let (Some(cause_id), Some(effect_id)) =
            (name_to_id.get(&cause_key), name_to_id.get(&effect_key))
        {
            let link = CausalLink {
                id: format!("causal_link:{}", uuid::Uuid::now_v7()),
                cause_id: cause_id.clone(),
                effect_id: effect_id.clone(),
                relationship: rel.relationship.clone(),
                strength: rel.strength,
                lag_days: rel.lag_days,
                evidence_node_ids: node_ids.iter().take(5).cloned().collect(),
                confidence: rel.confidence,
            };
            if insert_link(db, link).await.is_ok() {
                links_created += 1;
            }
        }
    }

    Ok(format!(
        "Built causal model: {} entities, {} links from {} source nodes",
        entities_created,
        links_created,
        nodes.len()
    ))
}

// =========================================================================
// SCENARIO SIMULATION
// =========================================================================

/// Simulate a scenario by parsing the trigger, tracing causal chains forward,
/// and predicting cascading effects.
pub async fn simulate_scenario(
    db: &Arc<BrainDb>,
    trigger: &str,
) -> Result<CausalPrediction, BrainError> {
    // 1. Load the current world model
    let entities = get_all_entities(db).await?;
    let links = get_all_links(db).await?;

    if entities.is_empty() {
        return Ok(CausalPrediction {
            trigger: trigger.to_string(),
            predicted_effects: Vec::new(),
            confidence: 0.0,
        });
    }

    // 2. Build adjacency map (cause_id → list of links)
    let mut adjacency: HashMap<String, Vec<&CausalLink>> = HashMap::new();
    for link in &links {
        adjacency
            .entry(link.cause_id.clone())
            .or_default()
            .push(link);
    }

    // 3. Build entity name→id lookup
    let name_to_id: HashMap<String, String> = entities
        .iter()
        .map(|e| (e.name.to_lowercase(), e.id.clone()))
        .collect();
    let id_to_name: HashMap<String, String> =
        entities.iter().map(|e| (e.id.clone(), e.name.clone())).collect();

    // 4. Use LLM to parse the trigger into affected entities
    let entity_names: Vec<String> = entities.iter().map(|e| e.name.clone()).collect();
    let llm = crate::commands::ai::get_llm_client_deep(db);
    let parse_prompt = format!(
        "Given this trigger event: \"{}\"\n\n\
         And these known entities: {}\n\n\
         Which entity is most directly affected and in what direction? \
         Respond in JSON: {{\"entity\": \"name\", \"direction\": \"increase|decrease\", \"magnitude\": 0.5}}\n\
         Use exact entity names from the list. If none match, use the closest one.",
        trigger,
        entity_names.join(", ")
    );

    let parse_response = llm.generate(&parse_prompt, 200).await.map_err(|e| {
        BrainError::Internal(format!("LLM trigger parsing failed: {}", e))
    })?;

    // 5. Parse trigger entity
    let trigger_parsed = parse_trigger_response(&parse_response);

    // 6. Forward propagation through causal chains (BFS, max depth 4)
    let mut effects: Vec<PredictedEffect> = Vec::new();
    let initial_direction = trigger_parsed.direction.clone();
    let initial_magnitude = trigger_parsed.magnitude;

    if let Some(start_id) = name_to_id.get(&trigger_parsed.entity.to_lowercase()) {
        let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut queue: Vec<(String, String, f32, u32)> = Vec::new(); // (entity_id, direction, magnitude, cumulative_lag)
        queue.push((
            start_id.clone(),
            initial_direction.clone(),
            initial_magnitude,
            0,
        ));
        visited.insert(start_id.clone());

        let max_depth = 4;
        let mut depth = 0;

        while !queue.is_empty() && depth < max_depth {
            let mut next_queue = Vec::new();
            for (entity_id, direction, magnitude, lag) in &queue {
                if let Some(outgoing) = adjacency.get(entity_id) {
                    for link in outgoing {
                        if visited.contains(&link.effect_id) {
                            continue;
                        }
                        visited.insert(link.effect_id.clone());

                        // Determine effect direction based on relationship type and incoming direction
                        let effect_direction = match (direction.as_str(), link.relationship.as_str())
                        {
                            ("increase", "increases") | ("decrease", "decreases") => "increase",
                            ("increase", "decreases") | ("decrease", "increases") => "decrease",
                            ("increase", "enables") => "increase",
                            ("increase", "blocks") | ("decrease", "enables") => "decrease",
                            ("decrease", "blocks") => "increase",
                            _ => "increase",
                        };

                        let effect_magnitude = magnitude * link.strength;
                        let effect_lag = lag + link.lag_days;

                        if effect_magnitude > 0.05 {
                            let entity_name = id_to_name
                                .get(&link.effect_id)
                                .cloned()
                                .unwrap_or_else(|| link.effect_id.clone());

                            effects.push(PredictedEffect {
                                entity: entity_name,
                                property: "value".to_string(),
                                direction: effect_direction.to_string(),
                                magnitude: effect_magnitude,
                                timeframe_days: effect_lag,
                            });

                            next_queue.push((
                                link.effect_id.clone(),
                                effect_direction.to_string(),
                                effect_magnitude,
                                effect_lag,
                            ));
                        }
                    }
                }
            }
            queue = next_queue;
            depth += 1;
        }
    }

    // 7. Compute overall confidence
    let avg_confidence = if effects.is_empty() {
        0.0
    } else {
        let link_confidences: Vec<f32> = links.iter().map(|l| l.confidence).collect();
        link_confidences.iter().sum::<f32>() / link_confidences.len() as f32
    };

    // 8. Store prediction
    let prediction_id = format!("prediction:{}", uuid::Uuid::now_v7());
    let now = chrono::Utc::now().to_rfc3339();
    let max_timeframe = effects.iter().map(|e| e.timeframe_days).max().unwrap_or(30);
    let due = chrono::Utc::now()
        + chrono::Duration::days(max_timeframe as i64);
    let prediction_text = format!(
        "Trigger: {}. {} predicted effects.",
        trigger,
        effects.len()
    );
    let causal_chain_json =
        serde_json::to_string(&effects).unwrap_or_else(|_| "[]".to_string());
    let due_str = due.to_rfc3339();

    let pred_id = prediction_id.clone();
    db.with_conn(move |conn| {
        conn.execute(
            "INSERT INTO future_predictions \
             (id, prediction, confidence, timeframe_days, evidence_node_ids, \
              causal_chain, validated, invalidated, created_at, due_at) \
             VALUES (?1, ?2, ?3, ?4, '[]', ?5, 0, 0, ?6, ?7)",
            params![
                pred_id,
                prediction_text,
                avg_confidence,
                max_timeframe,
                causal_chain_json,
                now,
                due_str
            ],
        )
        .map_err(|e| BrainError::Database(e.to_string()))?;
        Ok(())
    })
    .await?;

    Ok(CausalPrediction {
        trigger: trigger.to_string(),
        predicted_effects: effects,
        confidence: avg_confidence,
    })
}

// =========================================================================
// PARSING HELPERS
// =========================================================================

#[derive(Debug, Default)]
struct ExtractedModel {
    entities: Vec<ExtractedEntity>,
    relationships: Vec<ExtractedRelationship>,
}

#[derive(Debug)]
struct ExtractedEntity {
    name: String,
    entity_type: String,
    properties: HashMap<String, f64>,
}

#[derive(Debug)]
struct ExtractedRelationship {
    cause: String,
    effect: String,
    relationship: String,
    strength: f32,
    lag_days: u32,
    confidence: f32,
}

/// Parse the LLM's JSON response for entity/relationship extraction.
fn parse_causal_extraction(response: &str) -> ExtractedModel {
    // Try to find JSON in the response
    let json_str = extract_json_block(response);

    let parsed: serde_json::Value = match serde_json::from_str(&json_str) {
        Ok(v) => v,
        Err(_) => return ExtractedModel::default(),
    };

    let mut model = ExtractedModel::default();

    // Parse entities
    if let Some(entities) = parsed.get("entities").and_then(|v| v.as_array()) {
        for e in entities {
            let name = e
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let entity_type = e
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("market_factor")
                .to_string();
            let properties: HashMap<String, f64> = e
                .get("properties")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();

            if !name.is_empty() {
                model.entities.push(ExtractedEntity {
                    name,
                    entity_type,
                    properties,
                });
            }
        }
    }

    // Parse relationships
    if let Some(rels) = parsed.get("relationships").and_then(|v| v.as_array()) {
        for r in rels {
            let cause = r
                .get("cause")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let effect = r
                .get("effect")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let relationship = r
                .get("relationship")
                .and_then(|v| v.as_str())
                .unwrap_or("increases")
                .to_string();
            let strength = r
                .get("strength")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.5) as f32;
            let lag_days = r
                .get("lag_days")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            let confidence = r
                .get("confidence")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.5) as f32;

            if !cause.is_empty() && !effect.is_empty() {
                model.relationships.push(ExtractedRelationship {
                    cause,
                    effect,
                    relationship,
                    strength,
                    lag_days,
                    confidence,
                });
            }
        }
    }

    model
}

#[derive(Debug)]
struct TriggerParsed {
    entity: String,
    direction: String,
    magnitude: f32,
}

/// Parse the LLM's JSON response for trigger entity identification.
fn parse_trigger_response(response: &str) -> TriggerParsed {
    let json_str = extract_json_block(response);
    let parsed: serde_json::Value = match serde_json::from_str(&json_str) {
        Ok(v) => v,
        Err(_) => {
            return TriggerParsed {
                entity: String::new(),
                direction: "increase".to_string(),
                magnitude: 0.5,
            }
        }
    };

    TriggerParsed {
        entity: parsed
            .get("entity")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        direction: parsed
            .get("direction")
            .and_then(|v| v.as_str())
            .unwrap_or("increase")
            .to_string(),
        magnitude: parsed
            .get("magnitude")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.5) as f32,
    }
}

/// Extract a JSON block from an LLM response that may contain markdown fencing.
fn extract_json_block(text: &str) -> String {
    // Try to find ```json ... ``` blocks first
    if let Some(start) = text.find("```json") {
        let after = &text[start + 7..];
        if let Some(end) = after.find("```") {
            return after[..end].trim().to_string();
        }
    }
    // Try to find ``` ... ``` blocks
    if let Some(start) = text.find("```") {
        let after = &text[start + 3..];
        if let Some(end) = after.find("```") {
            return after[..end].trim().to_string();
        }
    }
    // Try to find { ... } directly
    if let Some(start) = text.find('{') {
        if let Some(end) = text.rfind('}') {
            return text[start..=end].to_string();
        }
    }
    text.trim().to_string()
}
