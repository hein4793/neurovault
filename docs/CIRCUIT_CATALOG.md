# Circuit Catalog

Complete reference for NeuroVault's **36 autonomous circuits**. One fires every 20 minutes on a rotation that skips the last 3 to run.

> Looking to write a new circuit? See [CIRCUIT_GUIDE.md](CIRCUIT_GUIDE.md).
> This document is the reader's reference: what each circuit does, what it reads, what it writes, and how to tell when one is misbehaving.

**Rotation:** 20 min/cycle → each circuit runs ~2×/day
**Source of truth:** `ALL_CIRCUITS` array in `src-tauri/src/circuits.rs`

---

## Quick-glance table

| # | Circuit | Phase | Needs embeddings? | Needs LLM? |
|---|---------|-------|:-:|:-:|
| 1 | `meta_reflection` | 0 | ✓ | ✓ |
| 2 | `user_pattern_mining` | 0 | ✓ | ✓ |
| 3 | `cross_domain_fusion` | 0 | ✓ | · |
| 4 | `quality_recalc` | 0 | · | · |
| 5 | `self_synthesis` | 0 | ✓ | ✓ |
| 6 | `curiosity_gap_fill` | 0 | · | ✓ |
| 7 | `iq_boost` | 0 | ✓ | · |
| 8 | `compression_cycle` | 1 | ✓ | ✓ |
| 9 | `contradiction_detector` | 2 | ✓ | ✓ |
| 10 | `decision_memory_extractor` | 2 | ✓ | ✓ |
| 11 | `knowledge_synthesizer` | 2 | · | ✓ |
| 12 | `self_assessment` | 2 | · | · |
| 13 | `prediction_validator` | 2 | · | · |
| 14 | `hypothesis_tester` | 2 | · | ✓ |
| 15 | `code_pattern_extractor` | 2 | · | ✓ |
| 16 | `synapse_prune` | 4 | · | · |
| 17 | `fingerprint_synthesis` | Ω | · | ✓ |
| 18 | `internal_dialogue` | Ω | · | ✓ |
| 19 | `swarm_orchestrator` | Ω II | · | ✓ |
| 20 | `temporal_analysis` | Ω III | · | · |
| 21 | `causal_model_builder` | Ω III | · | ✓ |
| 22 | `scenario_simulator` | Ω III | · | · |
| 23 | `knowledge_compiler` | Ω IV | · | ✓ |
| 24 | `circuit_optimizer` | Ω IV | · | · |
| 25 | `capability_tracker` | Ω IV | · | · |
| 26 | `self_reflection` | Ω IX | · | ✓ |
| 27 | `attention_update` | Ω IX | · | · |
| 28 | `curiosity_v2` | Ω IX | · | ✓ |
| 29 | `federation_sync` | Ω VI | · | · |
| 30 | `cluster_health_check` | Ω VII | · | · |
| 31 | `economic_audit` | Ω VIII | · | ✓ |
| 32 | `session_summarizer` | DB | · | ✓ |
| 33 | `context_quality_optimizer` | DB | · | · |
| 34 | `anticipatory_preloader` | DB | ✓ | · |
| 35 | `deep_synthesis` | DB | · | ✓ |
| 36 | `morning_briefing` | DB | · | · |

---

## Phase 0 — Core Self-Improvement

### 1. `meta_reflection`
**Purpose:** Critique the brain's recent activity and queue research missions for weak areas.
**Reads:** `autonomy_circuit_log`, `mcp_call_log`, `nodes` (recent), `research_missions`.
**Writes:** New `research_missions` rows; `insight` nodes tagged with `meta`.
**Known failure:** `Embedding error: Ollama returned 404 Not Found` — `nomic-embed-text` not pulled.

### 2. `user_pattern_mining`
**Purpose:** Extract behavioral rules ("user prefers X over Y") from chat history and auto-imports.
**Reads:** `nodes` with `source_type='chat'`, `user_cognition`.
**Writes:** `user_cognition` entries with `pattern_type`, confidence, times_confirmed.
**Known failure:** Same embedding 404 as above.

### 3. `cross_domain_fusion`
**Purpose:** Aggressively bridge concepts across domains (similarity > 0.45).
**Reads:** `nodes` across domains, `embeddings`.
**Writes:** `edges` with `relation_type='cross_domain_link'`.
**Notes:** Higher-recall counterpart to `iq_boost`.

### 4. `quality_recalc`
**Purpose:** Rescore quality and decay on sampled node batches.
**Reads:** `nodes` (sampled 10K at a time), `edges`.
**Writes:** Updates `nodes.quality_score`, `nodes.decay_score`.
**Typical duration:** < 3 s.

### 5. `self_synthesis`
**Purpose:** Cluster related nodes and produce `insight` / `hypothesis` nodes summarizing them.
**Reads:** `nodes`, `embeddings`.
**Writes:** `synthesized_by_brain=1` nodes with type `insight` or `hypothesis`.
**Known failure:** Embedding 404.

### 6. `curiosity_gap_fill`
**Purpose:** Pop topics off the curiosity queue and research them via LLM.
**Reads:** `research_missions`, `nodes`.
**Writes:** New `reference` nodes from LLM research output.
**Known failure:** Ollama generation model unavailable.

### 7. `iq_boost`
**Purpose:** High-precision cross-domain bridging (similarity > 0.6).
**Reads:** Same as `cross_domain_fusion`.
**Writes:** Same, but only high-confidence links.

---

## Phase 1 — Memory Compression

### 8. `compression_cycle`
**Purpose:** Consolidate redundant nodes by content hash + semantic similarity. Frees space and reduces noise.
**Reads:** `nodes`, `embeddings`, `compression_log`.
**Writes:** Merges duplicates; archives originals; logs to `compression_log`.
**Known failure:** Embedding 404 (heaviest embedding user of all circuits).

---

## Phase 2 — Cognitive Capabilities

### 9. `contradiction_detector`
**Purpose:** Find nodes that assert opposing claims.
**Reads:** `nodes`, `embeddings`.
**Writes:** `edges` with `relation_type='contradicts'`; optional `contradiction` insight nodes.

### 10. `decision_memory_extractor`
**Purpose:** Extract past decisions ("chose A over B because C") from chat / notes.
**Reads:** `nodes` with decision-like text patterns.
**Writes:** `decision` nodes in `user_cognition`.
**Known failure:** Embedding 404.

### 11. `knowledge_synthesizer`
**Purpose:** LLM-driven synthesis of new conclusions from existing knowledge clusters.
**Reads:** High-quality `nodes` in dense graph neighborhoods.
**Writes:** `synthesis` nodes.

### 12. `self_assessment`
**Purpose:** Score the brain's strengths and weaknesses per domain (numeric rubric, no LLM).
**Reads:** `nodes`, `edges`, `embeddings` (density / coverage).
**Writes:** Rows in `self_model` / related tables.
**Typical duration:** ~24 s (full scan).

### 13. `prediction_validator`
**Purpose:** Check past `prediction` nodes against outcomes.
**Reads:** `nodes` with type `prediction`, newer reference nodes.
**Writes:** `prediction` node status (validated / falsified / pending).

### 14. `hypothesis_tester`
**Purpose:** Generate tests for `hypothesis` nodes and mark results.
**Reads:** `nodes` with type `hypothesis`.
**Writes:** Test plan text on hypothesis nodes; outcomes on re-run.

### 15. `code_pattern_extractor`
**Purpose:** Extract reusable code patterns from ingested source files.
**Reads:** `nodes` with `source_type='code'`.
**Writes:** `pattern` nodes in `pattern` domain.

---

## Phase 4 — Graph Optimization

### 16. `synapse_prune`
**Purpose:** Remove weak edges (low strength, low traversal count, old).
**Reads:** `edges`, `synapse_prune_log`.
**Writes:** Deletes `edges`; logs pruned counts to `synapse_prune_log`.

---

## Phase Omega — Digital Twin

### 17. `fingerprint_synthesis`
**Purpose:** Build / refresh the `CognitiveFingerprint` (style, preferences, domains of expertise).
**Reads:** `user_cognition`, high-signal chat nodes.
**Writes:** Fingerprint row(s); callable via `/brain/fingerprint` endpoint.

### 18. `internal_dialogue`
**Purpose:** Advocate / Critic / Synthesizer debate on a topic; surface non-obvious trade-offs.
**Reads:** Seed topic (recent research mission or node cluster).
**Writes:** `debate` insight node with three-way transcript.

---

## Phase Omega II — Agent Swarm

### 19. `swarm_orchestrator`
**Purpose:** Dispatch pending goals to specialist agents (coder / analyst / researcher / planner / auditor).
**Reads:** `swarm_goals`, `swarm_tasks`.
**Writes:** Agent task results; updated goal progress.

---

## Phase Omega III — World Model

### 20. `temporal_analysis`
**Purpose:** Detect cycles, trends, anomalies across timestamps.
**Reads:** `nodes` ordered by `created_at`.
**Writes:** `pattern` entries (cycle / trend / anomaly).

### 21. `causal_model_builder`
**Purpose:** Infer causal relationships between `entity` nodes.
**Reads:** `entities`, co-occurrence in `nodes`.
**Writes:** `causal_links` with confidence.

### 22. `scenario_simulator`
**Purpose:** Run "what-if" propagations through the causal graph.
**Reads:** `causal_links`, `entities`.
**Writes:** `scenario` nodes with outcome branches.

---

## Phase Omega IV — Recursive Self-Improvement

### 23. `knowledge_compiler`
**Purpose:** Compile cognition into executable `if/then/always/never` rules.
**Reads:** `user_cognition`.
**Writes:** `knowledge_rules` table rows, retrievable via `/self/rules`.

### 24. `circuit_optimizer`
**Purpose:** Rank circuit performance (impact per second), tweak scheduling weights.
**Reads:** `autonomy_circuit_log`, `circuit_performance`.
**Writes:** Updates to `circuit_performance`.

### 25. `capability_tracker`
**Purpose:** Inventory current capabilities, identify gaps, bump proficiency scores.
**Reads:** Broad scan of `nodes`, `edges`, `knowledge_rules`.
**Writes:** Capability rows retrievable via `/self/capabilities`.

---

## Phase Omega IX — Consciousness Layer

### 26. `self_reflection`
**Purpose:** Daily self-reflection: what went well, what failed, what to change. Persists today's identity snapshot.
**Reads:** Last 24 h of `autonomy_circuit_log`, `iq_score` history.
**Writes:** `meta` domain node `Self-Reflection YYYY-MM-DD.md` in the vault.
**Typical duration:** ~35 s.

### 27. `attention_update`
**Purpose:** Refresh attention scores on 2000 candidate nodes; maintain top-100 focus window.
**Reads:** `nodes` (recent + high-quality).
**Writes:** `attention_score` column updates.

### 28. `curiosity_v2`
**Purpose:** Strategic (90%) + serendipitous (10%) research targeting based on information gain.
**Reads:** `research_missions`, capability gaps from `capability_tracker`.
**Writes:** Queues new missions; occasionally writes `reference` nodes directly.

---

## Phase Omega VI — Federation

### 29. `federation_sync`
**Purpose:** Exchange public knowledge with registered peer brains (opt-in, privacy-tiered).
**Reads:** `federation_peers`, local nodes tagged `privacy=public`.
**Writes:** Imported peer nodes (dedup by content hash); outbound share records.
**Notes:** Only runs if federation peers are configured. Otherwise returns instantly.

---

## Phase Omega VII — Infrastructure

### 30. `cluster_health_check`
**Purpose:** Check CPU, memory, disk, Ollama availability. Throttle on distress.
**Reads:** OS counters; `http://localhost:11434/api/tags`.
**Writes:** `health` entries; sets throttle flags that other circuits check.
**Typical duration:** < 1 s.

---

## Phase Omega VIII — Economic Autonomy

### 31. `economic_audit`
**Purpose:** Attribute tangible value (time saved, decisions supported) against compute cost.
**Reads:** `mcp_call_log`, `nodes` (value-attributed), `autonomy_circuit_log`.
**Writes:** `economics_ledger` rows; `/economics/report` reflects latest.

---

## Dual-Brain Intelligence Circuits

### 32. `session_summarizer`
**Purpose:** Extract structured intelligence from completed Claude Code sessions. Creates `session_summary` nodes with decisions, code written, problems solved, open questions, and next steps. Writes `~/.neurovault/export/session-handoff.md` for seamless cross-session continuity.
**Reads:** `~/.claude/projects/` — `.jsonl` session files not modified in >10 minutes.
**Writes:** `session_summary` nodes; `session-handoff.md` export file.
**Notes:** Only processes sessions that appear completed (no recent modification). Skips already-summarized sessions. Uses FAST LLM for extraction.

### 33. `context_quality_optimizer`
**Purpose:** Analyze how effective the sidekick's context bundles are over the past 7 days. If average knowledge nodes per bundle is too low, creates research missions to fill detected knowledge gaps.
**Reads:** `context_quality_log` table (populated by the sidekick on every inject cycle); `knowledge_gaps` table.
**Writes:** Updates to `knowledge_gaps`; new `research_missions` for high-priority gaps.
**Typical cadence:** Weekly (but will fire whenever the rotation selects it).

### 34. `anticipatory_preloader`
**Purpose:** Predict what context the user will need next and pre-build context bundles. Uses three prediction strategies: recently active topics, time-of-day patterns (morning/afternoon/evening), and pending research missions.
**Reads:** Recent `nodes.accessed_at`, `research_missions`, system clock.
**Writes:** `~/.neurovault/export/anticipatory-context.md` with pre-rendered context bundles.
**Notes:** Pre-builds up to 2 context bundles per cycle.

### 35. `deep_synthesis`
**Purpose:** 5-pass overnight reasoning on the day's top nodes. Identifies core themes, cross-topic connections, novel insights, predictions, and the single most important takeaway.
**Reads:** Top 10 high-quality nodes created in the past 18 hours.
**Writes:** `insight` node in `synthesis` domain with tags `dream`, `overnight`.
**Time gate:** Only runs between 22:00 and 06:00 local time. Skips silently outside this window.
**Notes:** Uses DEEP LLM (Qwen 32B) for multi-pass reasoning.

### 36. `morning_briefing`
**Purpose:** Compile overnight circuit results and new brain-generated nodes into a morning briefing before the user starts work.
**Reads:** `autonomy_circuit_log` (past 10 hours); `nodes` created overnight with `synthesized_by_brain=1`.
**Writes:** `~/.neurovault/export/morning-briefing.md`; `insight` node in `meta` domain.
**Time gate:** Only runs between 05:00 and 08:00 local time. Only once per day (deduplicates by date).

---

## Troubleshooting circuits

### "Ollama returned 404 Not Found"
The embedding model `nomic-embed-text` or the generation model is missing.
```bash
ollama pull nomic-embed-text
ollama pull qwen2.5-coder:14b
```
Affected circuits: `meta_reflection`, `user_pattern_mining`, `self_synthesis`, `compression_cycle`, `decision_memory_extractor` (all embedding-heavy).

### Circuit keeps skipping
Check `autonomy_state.last_result` — a circuit may be intentionally skipping if its input set is empty (e.g. `federation_sync` with no peers configured).
```sql
SELECT task_name, last_run_at, last_result FROM autonomy_state ORDER BY last_run_at DESC;
```

### Circuit exceeds 5-minute budget
See `circuit_optimizer` output and `circuit_performance` table. Either sample smaller batches in the circuit body, or split the work across multiple runs.

### Finding the last failure
```sql
SELECT started_at, circuit_name, status, substr(result,1,200) AS summary
FROM autonomy_circuit_log
WHERE status != 'ok'
ORDER BY started_at DESC
LIMIT 10;
```

---

## Adding a circuit

See [CIRCUIT_GUIDE.md](CIRCUIT_GUIDE.md) for the authoring walkthrough and [CONTRIBUTING.md](../CONTRIBUTING.md) for PR conventions.
