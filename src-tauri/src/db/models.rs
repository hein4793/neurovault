// Many of the constants and structs below are intentional public API
// surface (taxonomies for node/edge types, log structs constructed via
// Serde from raw DB queries) that the dead-code analyser can't see uses
// for. Suppress at the file level rather than tagging each one.
#![allow(dead_code)]
use serde::{Deserialize, Serialize};

// =========================================================================
// COGNITIVE TAXONOMY — Master Plan Phase 0
// =========================================================================

// --- Layer 1 (raw memory) node types ---
pub const NODE_TYPE_REFERENCE: &str = "reference";
pub const NODE_TYPE_CONCEPT: &str = "concept";
pub const NODE_TYPE_CONVERSATION: &str = "conversation";
pub const NODE_TYPE_CODE_SNIPPET: &str = "code_snippet";

// --- Layer 2 (semantic) node types ---
pub const NODE_TYPE_RESEARCH: &str = "research";
pub const NODE_TYPE_SUMMARY_CLUSTER: &str = "summary_cluster";

// --- Layer 3 (cognition / thinking) node types ---
pub const NODE_TYPE_HYPOTHESIS: &str = "hypothesis";
pub const NODE_TYPE_INSIGHT: &str = "insight";
pub const NODE_TYPE_DECISION: &str = "decision";
pub const NODE_TYPE_STRATEGY: &str = "strategy";
pub const NODE_TYPE_CONTRADICTION: &str = "contradiction";
pub const NODE_TYPE_PREDICTION: &str = "prediction";
pub const NODE_TYPE_SYNTHESIS: &str = "synthesis";

/// All thinking-node types in one slice (for filtering / iteration).
pub const THINKING_NODE_TYPES: &[&str] = &[
    NODE_TYPE_HYPOTHESIS,
    NODE_TYPE_INSIGHT,
    NODE_TYPE_DECISION,
    NODE_TYPE_STRATEGY,
    NODE_TYPE_CONTRADICTION,
    NODE_TYPE_PREDICTION,
    NODE_TYPE_SYNTHESIS,
];

// =========================================================================
// EDGE / RELATION TAXONOMY
// =========================================================================

// --- Layer 1 (structural) edge types ---
pub const EDGE_PART_OF: &str = "part_of";
pub const EDGE_SAME_TOPIC: &str = "same_topic";
pub const EDGE_SHARED_TAGS: &str = "shared_tags";
pub const EDGE_SAME_DOMAIN: &str = "same_domain";

// --- Layer 2 (semantic) edge types ---
pub const EDGE_CROSS_DOMAIN: &str = "cross_domain";

// --- Layer 3 (reasoning) edge types ---
pub const EDGE_CAUSES: &str = "causes";
pub const EDGE_CONTRADICTS: &str = "contradicts";
pub const EDGE_IMPROVES: &str = "improves";
pub const EDGE_DEPENDS_ON: &str = "depends_on";
pub const EDGE_DERIVED_FROM: &str = "derived_from";
pub const EDGE_SUPERSEDES: &str = "supersedes";
pub const EDGE_EVIDENCE_FOR: &str = "evidence_for";
pub const EDGE_EVIDENCE_AGAINST: &str = "evidence_against";

/// All reasoning edge types in one slice.
pub const REASONING_EDGE_TYPES: &[&str] = &[
    EDGE_CAUSES,
    EDGE_CONTRADICTS,
    EDGE_IMPROVES,
    EDGE_DEPENDS_ON,
    EDGE_DERIVED_FROM,
    EDGE_SUPERSEDES,
    EDGE_EVIDENCE_FOR,
    EDGE_EVIDENCE_AGAINST,
];

// =========================================================================
// MEMORY TIERS — Phase 2 (Tiered Memory Architecture)
// =========================================================================
pub const MEMORY_TIER_HOT: &str = "hot";
pub const MEMORY_TIER_WARM: &str = "warm";
pub const MEMORY_TIER_COLD: &str = "cold";

// =========================================================================
// CORE TYPES
// =========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeNode {
    pub id: Option<String>,
    pub title: String,
    pub content: String,
    pub summary: String,
    pub content_hash: String,
    pub domain: String,
    pub topic: String,
    pub tags: Vec<String>,
    pub node_type: String,
    pub source_type: String,
    pub source_url: Option<String>,
    pub source_file: Option<String>,
    pub quality_score: f64,
    pub visual_size: f64,
    pub cluster_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub accessed_at: String,
    pub access_count: u64,
    pub decay_score: f64,
    pub embedding: Option<Vec<f64>>,

    // --- Phase 0 cognitive extensions ---
    #[serde(default)]
    pub synthesized_by_brain: bool,
    #[serde(default)]
    pub cognitive_type: Option<String>,
    #[serde(default)]
    pub confidence: Option<f32>,
    #[serde(default)]
    pub memory_tier: Option<String>,
    #[serde(default)]
    pub compression_parent: Option<String>,
    #[serde(default)]
    pub brain_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeEdge {
    pub id: Option<String>,
    pub source_id: String,
    pub target_id: String,
    pub relation_type: String,
    pub strength: f64,
    pub discovered_by: String,
    pub evidence: String,
    pub animated: bool,
    pub created_at: String,
    pub traversal_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateNodeInput {
    pub title: String,
    pub content: String,
    pub domain: String,
    pub topic: String,
    pub tags: Vec<String>,
    pub node_type: String,
    pub source_type: String,
    pub source_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateEdgeInput {
    pub source_id: String,
    pub target_id: String,
    pub relation_type: String,
    pub evidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub title: String,
    pub content: String,
    pub summary: String,
    pub domain: String,
    pub topic: String,
    pub tags: Vec<String>,
    pub node_type: String,
    pub source_type: String,
    pub visual_size: f64,
    pub access_count: u64,
    pub decay_score: f64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    pub relation_type: String,
    pub strength: f64,
    pub animated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainStats {
    pub total_nodes: u64,
    pub total_edges: u64,
    pub domains: Vec<DomainCount>,
    pub recent_nodes: Vec<GraphNode>,
    pub total_sources: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainCount {
    pub domain: String,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoLinkResult {
    pub created: u64,
    pub existing: u64,
    pub total_nodes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub node: GraphNode,
    pub score: f64,
    pub matched_field: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CountResult {
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedNodes {
    pub nodes: Vec<GraphNode>,
    pub total: u64,
    pub offset: u64,
    pub limit: u64,
}

// =========================================================================
// PHASE 0 — COGNITIVE TABLES
// =========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserCognition {
    pub id: Option<String>,
    pub timestamp: String,
    pub trigger_node_ids: Vec<String>,
    pub pattern_type: String,
    pub extracted_rule: String,
    pub structured_rule: Option<String>,
    pub confidence: f32,
    pub times_confirmed: u32,
    pub times_contradicted: u32,
    pub embedding: Option<Vec<f64>>,
    pub linked_to_nodes: Vec<String>,
}

// =========================================================================
// PHASE 0 — AUTONOMY CIRCUIT TRACKING
// =========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutonomyCircuitLog {
    pub id: Option<String>,
    pub circuit_name: String,
    pub started_at: String,
    pub duration_ms: u64,
    pub status: String,
    pub result: String,
    pub details: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutonomyCircuitRotation {
    pub id: Option<String>,
    pub recent_circuits: Vec<String>,
    pub updated_at: String,
}

// =========================================================================
// PHASE 4.1 — Multi-brain support
// =========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Brain {
    pub id: Option<String>,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub color: String,
    pub created_at: String,
}

pub const MAIN_BRAIN: &str = "main";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveBrainState {
    pub id: Option<String>,
    pub active_brain_slug: String,
    pub updated_at: String,
}

// =========================================================================
// PHASE 4.4 — Per-domain IQ
// =========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainIq {
    pub domain: String,
    pub iq: f64,
    pub node_count: u64,
    pub avg_quality: f64,
    pub top_topics: Vec<String>,
}
