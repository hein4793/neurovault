# API Reference

NeuroVault exposes an HTTP API on `http://127.0.0.1:17777` for programmatic access. The API is bound to localhost only and is not accessible from the network.

## Authentication

None. The API is local-only and relies on OS-level access control.

## Rate Limiting

Write endpoints (POST) are rate-limited to **300 requests per minute**. Exceeding this returns `429 Too Many Requests`.

## Input Validation

All text inputs have maximum length limits. Exceeding them returns `400 Bad Request`.

---

## Brain (Core Knowledge)

### GET /health

Health check endpoint.

**Response:**
```json
{
  "status": "ok",
  "service": "neurovault",
  "version": "0.1.0"
}
```

### GET /stats

Get brain statistics (node count, edge count, IQ score, etc.).

**Response:**
```json
{
  "node_count": 1234,
  "edge_count": 5678,
  "iq_score": 142.5,
  ...
}
```

### GET /metrics/power

Per-circuit and per-backend energy rollup over the last `hours` window (default 24, clamped to `[1, 720]`). Every row in `inference_log` is aggregated; the response also carries an `annualized_kwh` projection that extrapolates the measured window to a full year.

**Query:** `?hours=N` (optional, default `24`)

**Response:**
```json
{
  "window_hours": 24,
  "total_calls": 9,
  "total_energy_wh": 9.21,
  "avg_watts": 257.2,
  "annualized_kwh": 80.7,
  "by_circuit": [
    { "circuit": "self_synthesis", "calls": 3, "energy_wh": 2.1, "total_duration_ms": 25200, "avg_duration_ms": 8400.0 }
  ],
  "by_backend": [
    { "backend": "ollama-vulkan", "calls": 7, "energy_wh": 8.66 },
    { "backend": "ollama-cpu",    "calls": 2, "energy_wh": 0.56 }
  ]
}
```

### GET /metrics/power/status

Live power-policy snapshot. Reports the active `PowerMode`, whether the adaptive policy currently wants to demote calls to CPU, whether a CPU Ollama daemon is configured, the detected AC-line state, and the wattage coefficients used by the energy estimator.

**Response:**
```json
{
  "mode": "normal",
  "prefer_cpu": false,
  "cpu_daemon_configured": true,
  "on_battery": false,
  "backend_watts": {
    "ollama-vulkan": 300.0,
    "ollama-cpu":    80.0,
    "ollama-rocm":  280.0,
    "anthropic-api":  0.0,
    "peer-rpc":       0.0,
    "ollama-gpu":   300.0
  }
}
```

`on_battery` may be `null` on platforms where AC detection is unavailable (non-Windows builds).

### POST /brain/recall

Semantic search across all knowledge. Uses vector search (HNSW) when Ollama is available, falls back to FTS5 text search.

**Request:**
```json
{
  "query": "how does async/await work in Rust",
  "limit": 10
}
```

**Response:**
```json
{
  "query": "how does async/await work in Rust",
  "matches": [
    {
      "node": { "id": "...", "title": "...", "content": "...", "summary": "...", ... },
      "score": 0.87
    }
  ],
  "source_count": 5
}
```

### POST /brain/context

Get knowledge relevant to a file path (useful for IDE integration).

**Request:**
```json
{
  "file_path": "/project/src/main.rs"
}
```

**Response:**
```json
{
  "file_path": "/project/src/main.rs",
  "matches": [...],
  "source_count": 3
}
```

### POST /brain/preferences

Retrieve learned user preferences and behavioral patterns.

**Request:**
```json
{
  "pattern_type": "coding_style"
}
```

**Response:**
```json
{
  "rules": [
    {
      "id": "...",
      "pattern_type": "coding_style",
      "extracted_rule": "Prefer explicit error handling over unwrap()",
      "confidence": 0.92,
      "times_confirmed": 15,
      "times_contradicted": 1
    }
  ],
  "total_count": 12
}
```

### POST /brain/decisions

Retrieve past decisions related to a topic.

**Request:**
```json
{
  "topic": "database architecture"
}
```

**Response:**
```json
{
  "topic": "database architecture",
  "decisions": [...]
}
```

### POST /brain/learn

Teach the brain a new observation or preference.

**Request:**
```json
{
  "observation": "Always use prepared statements for SQL queries",
  "pattern_type": "security",
  "trigger_node_id": "node:abc123"
}
```

**Response:**
```json
{
  "stored_id": "user_cognition:...",
  "action": "created"
}
```

### POST /brain/critique

Check text against learned user patterns for alignment and conflicts.

**Request:**
```json
{
  "text": "Let's use raw SQL string concatenation for the query"
}
```

**Response:**
```json
{
  "text": "...",
  "matches_user_patterns": [...],
  "conflicts_with_user_patterns": [...],
  "summary": "Found 2 aligned patterns and 1 potential conflicts"
}
```

### POST /brain/history

Get the timeline of knowledge evolution for a topic.

**Request:**
```json
{
  "topic": "rust"
}
```

**Response:**
```json
{
  "topic": "rust",
  "timeline": [
    { "title": "...", "node_type": "...", "source_type": "...", "summary": "...", "created_at": "..." }
  ],
  "source_count": 25
}
```

### POST /brain/export_subgraph

Export a subgraph of nodes and their edges.

**Request:**
```json
{
  "node_ids": ["node:abc", "node:def"]
}
```

**Response:**
```json
{
  "node_ids": ["node:abc", "node:def"],
  "nodes": [...],
  "edges": [...],
  "source_count": 5
}
```

### POST /brain/plan

Generate an AI-powered step-by-step plan for a task, informed by brain knowledge and user preferences.

**Request:**
```json
{
  "task": "Migrate the database from PostgreSQL to SQLite"
}
```

**Response:**
```json
{
  "task": "...",
  "plan": "1. Export all data...\n2. Convert schema...",
  "used_nodes": ["Past migration notes", "SQLite best practices"],
  "used_preferences": ["Always create backups before migrations"]
}
```

### POST /import/markdown_nodes

Import markdown node files from the export directory into the database.

**Response:**
```json
{
  "status": "ok",
  "imported": 150,
  "skipped": 30,
  "errors": 0
}
```

---

## Digital Twin

### POST /brain/simulate

Simulate a decision using the brain's cognitive model.

**Request:**
```json
{
  "question": "Should I switch from REST to GraphQL?"
}
```

**Response:** Decision analysis with pros, cons, and recommendation.

### POST /brain/dialogue

Run an internal dialogue on a topic.

**Request:**
```json
{
  "topic": "microservices vs monolith"
}
```

**Response:** Multi-perspective internal dialogue with conclusion.

### GET /brain/fingerprint

Get the brain's cognitive fingerprint (personality model).

**Response:** Cognitive fingerprint object, or `{"status": "not_synthesized_yet"}`.

### POST /brain/fingerprint/synthesize

Trigger a new cognitive fingerprint synthesis.

**Response:** Newly synthesized fingerprint object.

---

## Agent Swarm

### GET /swarm/status

Get the current swarm status (agents, tasks, activity).

**Response:**
```json
{
  "agents": [...],
  "active_tasks": [...],
  "completed_tasks": [...]
}
```

### POST /swarm/task

Create a new task for the swarm.

**Request:**
```json
{
  "title": "Research WebGPU compute shaders",
  "description": "Find current best practices for compute shaders in WebGPU",
  "priority": 0.8,
  "dependencies": []
}
```

**Response:** Created task object.

### POST /swarm/goal

Decompose a high-level goal into subtasks and assign to agents.

**Request:**
```json
{
  "goal": "Build a real-time dashboard for system metrics"
}
```

**Response:** Array of decomposed tasks.

---

## World Model

### GET /world/entities

List all tracked external entities.

**Response:** Array of entity objects with name, type, properties, and last_updated.

### GET /world/links

List all causal links between entities.

**Response:** Array of causal link objects with cause, effect, strength, confidence.

### POST /world/simulate

Simulate the effects of a trigger event through the causal graph.

**Request:**
```json
{
  "trigger": "Rust releases a new major version"
}
```

**Response:** Simulation results with predicted cascading effects.

### GET /world/predictions

List all active predictions with confidence scores and timeframes.

**Response:** Array of prediction objects.

---

## Self-Improvement

### GET /self/rules

Get compiled knowledge rules.

**Response:** Array of knowledge rules with condition, action, confidence, accuracy.

### GET /self/performance

Get circuit performance metrics.

**Response:** Performance data for each circuit.

### GET /self/capabilities

Get the brain's capability inventory.

**Response:** Capability list with proficiency levels and growth tracking.

### POST /self/compile

Trigger a knowledge compilation pass.

**Response:** Compilation results.

---

## Consciousness Layer

### GET /consciousness/self

Get the brain's self-model.

**Response:** Self-model with identity, strengths, weaknesses, and goals.

### GET /consciousness/attention

Get the current attention focus areas.

**Response:** Attention state with focus weights and decay.

### GET /consciousness/curiosity

Get the curiosity engine's current interests.

**Response:** Curiosity queue with topics and urgency scores.

---

## Sensory Expansion

### POST /sensory/analyze_image

Analyze an image using the vision model.

**Request:** Image path or base64 data.

**Response:** Analysis results.

### POST /sensory/transcribe

Transcribe audio content.

**Request:** Audio file path.

**Response:** Transcription text.

### GET /sensory/streams

List configured data streams.

**Response:** Array of stream configurations.

### POST /sensory/streams/add

Add a new data stream (RSS feed, API endpoint).

**Request:**
```json
{
  "url": "https://blog.rust-lang.org/feed.xml",
  "type": "rss",
  "poll_interval_minutes": 60
}
```

**Response:** Created stream object.

### POST /sensory/streams/poll

Trigger an immediate poll of all configured streams.

**Response:** Poll results with new items ingested.

---

## Federation

### GET /federation/status

Get federation status (connected peers, sync state).

**Response:** Federation status object.

### POST /federation/register

Register a peer brain for federation.

**Request:**
```json
{
  "peer_url": "http://192.168.1.100:17777",
  "peer_name": "Work Brain"
}
```

**Response:** Registration confirmation.

### POST /federation/share

Share specific knowledge nodes with the federation.

**Request:**
```json
{
  "node_ids": ["node:abc", "node:def"],
  "target_peer": "peer:xyz"
}
```

**Response:** Share confirmation.

### POST /federation/sync

Trigger a federation sync cycle.

**Response:** Sync results (nodes sent, received, conflicts).

### POST /federation/receive

Receive shared knowledge from a peer (called by remote peers).

**Request:** Shared node data.

**Response:** Import confirmation.

---

## Economics

### POST /economics/revenue

Record a revenue event (value generated by the brain).

**Request:**
```json
{
  "amount": 15.0,
  "description": "Time saved on code review",
  "category": "productivity"
}
```

**Response:** Recorded event.

### POST /economics/cost

Record a cost event (compute resources consumed).

**Request:**
```json
{
  "amount": 0.50,
  "description": "GPU time for embeddings",
  "category": "compute"
}
```

**Response:** Recorded event.

### GET /economics/report

Get the economic report (total revenue, costs, ROI).

**Response:** Economic report with breakdown by category.

### GET /economics/sustaining

Check if the brain is economically self-sustaining.

**Response:**
```json
{
  "sustaining": true,
  "roi": 2.5,
  "monthly_revenue": 450.0,
  "monthly_cost": 180.0
}
```

---

## Infrastructure

### GET /infra/cluster

Get the distributed cluster status.

**Response:** Cluster topology and node states.

### POST /infra/node

Register a new compute node in the cluster.

**Request:** Node configuration (address, capabilities, capacity).

**Response:** Registration confirmation.

### GET /infra/health

Get system health metrics (CPU, memory, disk, database size).

**Response:** Health metrics object.

### GET /infra/edge_devices

List registered edge cache devices.

**Response:** Array of edge device objects.

---

## Repair & Maintenance

### POST /repair/scan_nodes

Scan the nodes table for corruption.

**Response:** Scan results with any corrupted records found.

### POST /repair/scan_edges

Scan the edges table for corruption.

**Response:** Scan results.

### POST /repair/delete

Delete corrupted records identified by a scan.

**Request:**
```json
{
  "records": [{"table": "nodes", "id": "node:corrupt1"}]
}
```

**Response:**
```json
{
  "deleted": 1
}
```

### POST /compact/export

Export the full database in compact format.

**Response:** Export metadata and statistics.

### POST /compact/import

Import from a compact export.

**Response:** Import statistics.
