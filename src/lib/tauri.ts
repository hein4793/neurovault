import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";

// ===== Core Types =====

export interface GraphNode {
  id: string;
  title: string;
  content: string;
  summary: string;
  domain: string;
  topic: string;
  tags: string[];
  node_type: string;
  source_type: string;
  visual_size: number;
  access_count: number;
  decay_score: number;
  created_at: string;
}

export interface GraphEdge {
  id: string;
  source: string;
  target: string;
  relation_type: string;
  strength: number;
  animated: boolean;
}

export interface SearchResult {
  node: GraphNode;
  score: number;
  matched_field: string;
}

export interface BrainStats {
  total_nodes: number;
  total_edges: number;
  domains: { domain: string; count: number }[];
  recent_nodes: GraphNode[];
  total_sources: number;
}

export interface ForceGraphNode extends GraphNode {
  x?: number; y?: number; z?: number;
  fx?: number; fy?: number; fz?: number;
  __threeObj?: any;
}

export interface ForceGraphLink {
  id: string;
  source: string | ForceGraphNode;
  target: string | ForceGraphNode;
  relation_type: string;
  strength: number;
  animated: boolean;
}

export interface BrainSettings {
  ollama_url: string;
  embedding_model: string;
  auto_sync_enabled: boolean;
  data_dir: string;
  llm_provider: string;
  llm_model: string;
  // Phase 2.6 — multi-model routing
  llm_model_fast?: string;
  llm_model_deep?: string;
  // Autonomy intervals
  autonomy_enabled?: boolean;
  autonomy_linking_mins?: number;
  autonomy_quality_mins?: number;
  autonomy_learning_mins?: number;
  autonomy_export_mins?: number;
  autonomy_max_daily_research?: number;
}

export interface AutoLinkResult { created: number; existing: number; total_nodes: number; }

// ===== Embedding Types =====
export interface EmbeddingStats { total_nodes: number; embedded_nodes: number; pending_nodes: number; ollama_connected: boolean; }
export interface SimilarNode { node: GraphNode; similarity: number; }

// ===== Quality Types =====
export interface QualityReport { total_nodes: number; avg_quality: number; avg_decay: number; low_quality_count: number; high_quality_count: number; decayed_count: number; }
export interface DuplicatePair { node_a: GraphNode; node_b: GraphNode; similarity: number; recommendation: string; }

// ===== AI Types =====
export interface BrainAnswer { answer: string; sources: GraphNode[]; confidence: number; }

// ===== Learning Types =====
export interface KnowledgeGap { topic: string; reason: string; priority: number; domain: string; }
export interface CuriosityItem { topic: string; reason: string; priority: number; source: string; }
export interface ResearchMission { id: string; topic: string; status: string; nodes_created: number; started_at: string; completed_at: string | null; summary: string | null; }

// ===== Analysis Types =====
export interface HubNode { node: GraphNode; connection_count: number; }
export interface BridgeNode { node: GraphNode; connects_domains: string[]; }
export interface ClusterInfo { domain: string; node_count: number; avg_quality: number; }
export interface PatternReport { hubs: HubNode[]; bridges: BridgeNode[]; islands: GraphNode[]; clusters: ClusterInfo[]; }
export interface DayCount { date: string; count: number; }
export interface DomainGrowth { domain: string; count: number; recent_count: number; }
export interface TopicHeat { topic: string; score: number; node_count: number; }
export interface BrainIqBreakdown {
  // Foundation tier (0-100)
  quality: number; connectivity: number; freshness: number;
  diversity: number; coverage: number; volume: number;
  // Intelligence tier (0-100)
  depth: number; cross_domain: number; semantic: number;
  research_ratio: number; coherence: number; high_quality_pct: number;
  // Meta-Intelligence tier (0-100) — Phase 2.4
  self_improvement_velocity?: number;
  prediction_accuracy?: number;
  novel_insight_rate?: number;
  autonomy_independence?: number;
  user_model_accuracy?: number;
}
export interface TrendReport {
  growth_by_day: DayCount[];
  domain_growth: DomainGrowth[];
  hot_topics: TopicHeat[];
  forgotten_topics: TopicHeat[];
  brain_iq: number;
  iq_breakdown: BrainIqBreakdown;
  domain_iqs?: DomainIq[];
}
export interface Recommendation { rec_type: string; title: string; description: string; priority: number; node_id: string | null; }

// ===== Backup Types =====
export interface BackupInfo { filename: string; path: string; size_bytes: number; created_at: string; node_count: number; edge_count: number; }

// ===== MCP Types =====
export interface McpConnection { name: string; status: string; server_type: string; last_used: string | null; }
export interface McpStatus { connections: McpConnection[]; }

// ===== Event Types =====
export interface BrainEventPayload { type: string; id?: string; title?: string; source?: string; target?: string; query?: string; results?: number; nodes_created?: number; nodes_imported?: number; created?: number; total_nodes?: number; topic?: string; key?: string; error?: string; task?: string; result?: string; duration_ms?: number; topics_researched?: number; iq?: number; }

// ===== Colors =====
export const DOMAIN_COLORS: Record<string, string> = {
  technology: "#00A8FF", business: "#00CC88", research: "#8B5CF6",
  pattern: "#F59E0B", reference: "#94A3B8", personal: "#F97316", core: "#38BDF8",
};
export const DOMAINS = Object.keys(DOMAIN_COLORS);
export function getDomainColor(domain: string): string { return DOMAIN_COLORS[domain] || "#38BDF8"; }

// ===== Graph Commands =====
export const getAllNodes = () => invoke<GraphNode[]>("get_all_nodes");
export const getAllEdges = () => invoke<GraphEdge[]>("get_all_edges");
export const getNodeCount = () => invoke<number>("get_node_count");
export const getEdgeCount = () => invoke<number>("get_edge_count");
export const getNodesPaginated = (offset: number, limit: number) => invoke<GraphNode[]>("get_nodes_paginated", { offset, limit });
export const createNode = (params: { title: string; content: string; domain: string; topic: string; tags: string[]; nodeType: string; sourceType: string; sourceUrl?: string }) => invoke<GraphNode>("create_node", params);
export const updateNode = (params: { id: string; title?: string; content?: string; domain?: string; topic?: string; tags?: string[] }) => invoke<GraphNode>("update_node", params);
export const deleteNode = (id: string) => invoke<boolean>("delete_node", { id });
export const createEdge = (params: { sourceId: string; targetId: string; relationType: string; evidence: string }) => invoke<GraphEdge>("create_edge", params);
export const deleteEdge = (id: string) => invoke<boolean>("delete_edge", { id });
export const getEdgesForNode = (nodeId: string) => invoke<GraphEdge[]>("get_edges_for_node", { nodeId });

// ===== Search Commands =====
export const searchNodes = (query: string) => invoke<SearchResult[]>("search_nodes", { query });
export const semanticSearch = (query: string, limit?: number) => invoke<SearchResult[]>("semantic_search", { query, limit });

// ===== Ingestion Commands =====
export const ingestUrl = (url: string) => invoke<GraphNode[]>("ingest_url", { url });
export const ingestText = (params: { title: string; content: string; domain: string; topic: string }) => invoke<GraphNode>("ingest_text", params);
export const importAiMemory = () => invoke<GraphNode[]>("import_ai_memory");
export const importChatHistory = () => invoke<GraphNode[]>("import_chat_history");
export const researchTopic = (topic: string) => invoke<GraphNode[]>("research_topic", { topic });
export const researchBatch = (topics: string[]) => invoke<GraphNode[]>("research_batch", { topics });

// ===== Stats Commands =====
export const getBrainStats = () => invoke<BrainStats>("get_brain_stats");
export const autoLinkNodes = () => invoke<AutoLinkResult>("auto_link_nodes");

// ===== Settings Commands =====
export const getSettings = () => invoke<BrainSettings>("get_settings");
export const updateSettings = (settings: BrainSettings) => invoke<BrainSettings>("update_settings", { settings });
export const clearCache = () => invoke<string>("clear_cache");
export const getBrainVersion = () => invoke<string>("get_brain_version");

// ===== Phase 3.1 — Installed Ollama models =====
export interface InstalledModel {
  name: string;
  size: number;
  modified_at: string;
  family: string;
  size_label: string;
}
export interface InstalledModelsResponse {
  ollama_url: string;
  reachable: boolean;
  models: InstalledModel[];
}
export const listInstalledModels = () => invoke<InstalledModelsResponse>("list_installed_models");

// ===== Phase 4.1 — Multi-brain =====
export interface Brain {
  id: string | null;
  slug: string;
  name: string;
  description: string;
  color: string;
  created_at: string;
}
export interface BrainStatsRow {
  slug: string;
  total_nodes: number;
  total_edges: number;
}
export const listBrains = () => invoke<Brain[]>("list_brains");
export const createBrain = (slug: string, name: string, description?: string, color?: string) =>
  invoke<Brain>("create_brain", { slug, name, description, color });
export const deleteBrain = (slug: string) => invoke<boolean>("delete_brain", { slug });
export const getActiveBrain = () => invoke<string>("get_active_brain");
export const setActiveBrain = (slug: string) => invoke<string>("set_active_brain", { slug });
export const getBrainStatsFor = (slug: string) =>
  invoke<BrainStatsRow>("get_brain_stats_for", { slug });

// ===== Phase 4.4 — Per-domain IQ =====
export interface DomainIq {
  domain: string;
  iq: number;
  node_count: number;
  avg_quality: number;
  top_topics: string[];
}

// ===== Phase 4.7 — Brain activity =====
export interface CircuitLogEntry {
  circuit_name: string;
  started_at: string;
  duration_ms: number;
  status: string;
  result: string;
}
export interface MasterLoopEntry {
  started_at: string;
  health: string;
  new_nodes_24h: number;
  new_thinking_nodes_24h: number;
  thinking_ratio: number;
  missions_queued: number;
  insight_created: boolean;
  analysis_summary: string;
}
export interface MemoryTierEntry {
  ran_at: string;
  scanned: number;
  promoted_hot: number;
  promoted_warm: number;
  demoted_cold: number;
  already_correct: number;
}
export interface FineTuneEntry {
  timestamp: string;
  dataset_size_bytes: number;
  dataset_entries: number;
  status: string;
  script_path: string;
  prepared_at: string;
}
export interface CircuitHealth {
  circuit_name: string;
  success_count: number;
  fail_count: number;
  avg_duration_ms: number;
}
export interface BrainActivitySnapshot {
  recent_circuits: CircuitLogEntry[];
  recent_master_loops: MasterLoopEntry[];
  recent_memory_tier_passes: MemoryTierEntry[];
  pending_fine_tunes: FineTuneEntry[];
  circuit_health: CircuitHealth[];
  generated_at: string;
}
export const getBrainActivity = () => invoke<BrainActivitySnapshot>("get_brain_activity");

// ===== Embedding Commands =====
export const getEmbeddingStats = () => invoke<EmbeddingStats>("get_embedding_stats");
export const findSimilarNodes = (nodeId: string, limit?: number) => invoke<SimilarNode[]>("find_similar_nodes", { nodeId, limit });

// ===== Quality Commands =====
export const calculateQuality = () => invoke<[number, number]>("calculate_quality");
export const calculateDecay = () => invoke<[number, number]>("calculate_decay");
export const getQualityReport = () => invoke<QualityReport>("get_quality_report");
export const mergeDuplicateNodes = (keepId: string, removeId: string) => invoke<GraphNode>("merge_duplicate_nodes", { keepId, removeId });
export const enhanceQuality = () => invoke<[number, number, number]>("enhance_quality");
export const boostIq = () => invoke<[number, number, number]>("boost_iq");
export const deepLearn = () => invoke<[number, number]>("deep_learn");
export const qualitySweep = () => invoke<[number, number]>("quality_sweep");
export const maximizeIq = () => invoke<string>("maximize_iq");

// ===== AI Commands =====
export const askBrain = (question: string) => invoke<BrainAnswer>("ask_brain", { question });
export const summarizeNodeAi = (nodeId: string) => invoke<string>("summarize_node_ai", { nodeId });
export const backfillSummaries = () => invoke<[number, number]>("backfill_summaries");
export const extractTagsAi = (nodeId: string) => invoke<string[]>("extract_tags_ai", { nodeId });

// ===== Learning Commands =====
export const getKnowledgeGaps = () => invoke<KnowledgeGap[]>("get_knowledge_gaps");
export const getCuriosityQueue = () => invoke<CuriosityItem[]>("get_curiosity_queue");
export const createResearchMission = (topic: string) => invoke<ResearchMission>("create_research_mission", { topic });
export const getResearchMissions = () => invoke<ResearchMission[]>("get_research_missions");

// ===== Analysis Commands =====
export const analyzePatterns = () => invoke<PatternReport>("analyze_patterns");
export const analyzeTrends = () => invoke<TrendReport>("analyze_trends");
export const getRecommendations = () => invoke<Recommendation[]>("get_recommendations");

// ===== MCP Commands =====
export const getMcpStatus = () => invoke<McpStatus>("get_mcp_status");

// ===== Backup Commands =====
export const createBackup = () => invoke<BackupInfo>("create_backup");
export const listBackups = () => invoke<BackupInfo[]>("list_backups");
export const restoreBackup = (path: string) => invoke<[number, number]>("restore_backup", { path });
export const exportJson = (path: string) => invoke<number>("export_json", { path });
export const exportMarkdown = (dir: string) => invoke<number>("export_markdown", { dir });
export const exportCsv = (path: string) => invoke<number>("export_csv", { path });

// ===== Node Cloud (198K+ point rendering) =====
export interface NodeCloud {
  positions: number[];
  colors: number[];
  sizes: number[];
  count: number;
}
export const getNodeCloud = () => invoke<NodeCloud>("get_node_cloud");

// ===== Domain Clusters =====
export interface DomainCluster {
  domain: string;
  node_count: number;
  avg_quality: number;
  position: [number, number, number];
  color: [number, number, number];
}
export const getDomainClusters = () => invoke<DomainCluster[]>("get_domain_clusters");

// ===== Edge Bundles =====
export interface EdgeBundle {
  source_domain: string;
  target_domain: string;
  count: number;
}
export const getEdgeBundleCounts = () => invoke<EdgeBundle[]>("get_edge_bundle_counts");

// ===== Domain-filtered nodes =====
export const getNodesByDomain = (domain: string, limit: number) =>
  invoke<GraphNode[]>("get_nodes_by_domain", { domain, limit });

// ===== File Ingestion =====
export const ingestFiles = (paths: string[]) => invoke<GraphNode[]>("ingest_files", { paths });

// ===== User Profile =====
export interface UserProfile {
  top_domains: [string, number][];
  top_topics: [string, number][];
  primary_languages: string[];
  frameworks: string[];
  coding_patterns: string[];
  learning_velocity: number;
  total_nodes: number;
  total_interactions: number;
  last_synthesized: string;
}
export const getUserProfile = () => invoke<UserProfile>("get_user_profile");
export const synthesizeUserProfile = () => invoke<string>("synthesize_user_profile");

// ===== Training Data =====
export const generateTrainingDataset = (format: string, path: string) => invoke<number>("generate_training_dataset", { format, path });

// ===== Autonomy Commands =====

export interface AutonomyTask {
  name: string;
  last_run: string | null;
  last_result: string | null;
  status: string;
  runs_today: number;
}

export interface AutonomyTodayStats {
  topics_researched: number;
  nodes_created: number;
  links_made: number;
  quality_improved: number;
}

export interface AutonomyStatus {
  enabled: boolean;
  tasks: AutonomyTask[];
  today: AutonomyTodayStats;
}

export const getAutonomyStatus = () => invoke<AutonomyStatus>("get_autonomy_status");
export const setAutonomyEnabled = (enabled: boolean) => invoke<void>("set_autonomy_enabled", { enabled });
export const triggerAutonomyTask = (task: string) => invoke<string>("trigger_autonomy_task", { task });

// ===== Phase Omega — Digital Twin Types =====
export interface CognitiveFingerprint {
  risk_tolerance: number;
  decision_speed: number;
  analytical_depth: number;
  creativity: number;
  pattern_recognition: number;
  abstraction_level: number;
  detail_orientation: number;
  learning_agility: number;
  synthesized_at: string;
}
export interface DecisionSimulation {
  question: string;
  prediction: string;
  confidence: number;
  reasoning: string;
  alternatives: string[];
}
export interface DialogueTurn {
  role: string;
  content: string;
}
export interface InternalDialogue {
  topic: string;
  turns: DialogueTurn[];
  synthesis: string;
}
export const getCognitiveFingerprint = () => invoke<CognitiveFingerprint>("get_cognitive_fingerprint");
export const simulateDecision = (question: string) => invoke<DecisionSimulation>("simulate_decision", { question });
export const runDialogue = (topic: string) => invoke<InternalDialogue>("run_dialogue", { topic });
export const synthesizeFingerprint = () => invoke<CognitiveFingerprint>("synthesize_fingerprint");

// ===== Phase Omega — Agent Swarm Types =====
export interface SwarmAgent {
  name: string;
  capabilities: string[];
  autonomy_level: number;
  status: string;
  current_task: string | null;
}
export interface SwarmTask {
  id: string;
  description: string;
  assigned_agent: string | null;
  status: string;
  priority: number;
  created_at: string;
  completed_at: string | null;
  result: string | null;
}
export interface SwarmStatus {
  agents: SwarmAgent[];
  active_tasks: number;
  completed_tasks: number;
  total_goals_decomposed: number;
}
export interface GoalDecomposition {
  goal: string;
  tasks: SwarmTask[];
}
export const getSwarmStatus = () => invoke<SwarmStatus>("get_swarm_status");
export const getSwarmTasks = () => invoke<SwarmTask[]>("get_swarm_tasks");
export const decomposeGoal = (goal: string) => invoke<GoalDecomposition>("decompose_goal", { goal });

// ===== Phase Omega — World Model Types =====
export interface WorldEntity {
  id: string;
  name: string;
  entity_type: string;
  properties: Record<string, string>;
  created_at: string;
}
export interface CausalLink {
  id: string;
  cause: string;
  effect: string;
  strength: number;
  evidence_count: number;
}
export interface Prediction {
  id: string;
  prediction: string;
  confidence: number;
  timeframe: string;
  status: string;
  created_at: string;
  validated_at: string | null;
}
export interface ScenarioResult {
  trigger: string;
  predicted_effects: { effect: string; probability: number; timeframe: string }[];
}
export const getWorldEntities = () => invoke<WorldEntity[]>("get_world_entities");
export const getCausalLinks = () => invoke<CausalLink[]>("get_causal_links");
export const getPredictions = () => invoke<Prediction[]>("get_predictions");
export const simulateScenarioCmd = (trigger: string) => invoke<ScenarioResult>("simulate_scenario_cmd", { trigger });

// ===== Phase Omega — Self-Improvement Types =====
export interface KnowledgeRule {
  id: string;
  rule_type: string;
  condition: string;
  action: string;
  confidence: number;
  accuracy: number;
  times_applied: number;
}
export interface CircuitPerformance {
  circuit_name: string;
  total_runs: number;
  success_rate: number;
  avg_duration_ms: number;
  efficiency_score: number;
}
export interface Capability {
  name: string;
  proficiency: number;
  status: string;
  last_improved: string | null;
}
export const getKnowledgeRules = () => invoke<KnowledgeRule[]>("get_knowledge_rules");
export const getCircuitPerformance = () => invoke<CircuitPerformance[]>("get_circuit_performance");
export const getCapabilities = () => invoke<Capability[]>("get_capabilities");
export const compileRulesNow = () => invoke<string>("compile_rules_now");

// ===== Phase Omega — Consciousness Types =====
export interface SelfModel {
  identity: string;
  iq: number;
  strongest_domains: string[];
  weakest_domains: string[];
  bottleneck: string;
  priorities: string[];
  last_updated: string;
}
export interface AttentionNode {
  node_id: string;
  title: string;
  score: number;
  reason: string;
}
export interface CuriosityTargetV2 {
  topic: string;
  strategy: string;
  expected_gain: number;
  reason: string;
}
export interface LearningVelocity {
  domain: string;
  velocity: number;
  trend: string;
}
export const getSelfModel = () => invoke<SelfModel>("get_self_model");
export const getAttentionWindow = () => invoke<AttentionNode[]>("get_attention_window");
export const getCuriosityTargetsV2 = (limit: number) => invoke<CuriosityTargetV2[]>("get_curiosity_targets_v2", { limit });
export const getLearningVelocity = () => invoke<LearningVelocity[]>("get_learning_velocity");

// ===== Event Listener =====
export const onBrainEvent = (callback: (event: BrainEventPayload) => void): Promise<UnlistenFn> =>
  listen<BrainEventPayload>("brain-event", (e) => callback(e.payload));
