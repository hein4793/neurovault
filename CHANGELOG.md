# Changelog

## [Unreleased]

### Added — Power Management (Phases 1-6)
- **Per-inference energy telemetry** (`power_telemetry.rs`): Every LLM call logs one row to a new `inference_log` table with duration, tokens, backend tag, and estimated watt-hours. Circuit attribution via a `tokio::task_local!` that wraps `run_circuit()` dispatch — zero touches to the 36 individual circuit functions.
- **CPU backend routing**: Optional second Ollama daemon on a separate port runs CPU-only inference at ~80 W vs. the ~300 W GPU pool. Opt in via `NEUROVAULT_OLLAMA_CPU_URL` env var or the `ollama_cpu_url` config field.
- **Circuit profiles**: `Interactive` / `NearRealTime` / `Batch` classification drives automatic routing — scoped Batch circuits go to CPU when the daemon is configured, interactive paths stay on GPU.
- **Adaptive power policy** (`power_policy.rs`): `PowerMode` state machine (`normal` / `eco` / `idle_opportunistic` / `thermal_throttle` / `load_shed`) with a 30 s background loop that polls AC line status via `kernel32::GetSystemPowerStatus` and transitions `normal<->eco` on wall/battery changes. On battery, every call demotes to CPU.
- **Model tiering**: New `llm_model_cpu` setting (defaults to `qwen2.5:3b` ~2 GB) so CPU routes don't try to run a 14 B+ model at 1 tok/s.
- **Dashboard endpoints**:
  - `GET /metrics/power?hours=N` — per-circuit + per-backend rollup with `total_energy_wh`, `avg_watts`, `annualized_kwh`.
  - `GET /metrics/power/status` — live `PowerMode`, `on_battery`, `cpu_daemon_configured`, `prefer_cpu`, and the wattage coefficients used by the estimator.
- **Decode helper** (`lib.rs`): `decode_claude_project_name()` replaces three hardcoded folder-prefix `replace()` constants with a runtime helper that derives the encoded home prefix from `$HOME` — works on any user's folder layout regardless of username.
- Full plan and activation checklist: [`docs/POWER_PLAN.md`](docs/POWER_PLAN.md).

### Verified
End-to-end on a reference box (i7-12700F + RX 6900 XT + 64 GB, Windows 11): the first `ollama-cpu` call logged at 0.28 Wh versus the `ollama-vulkan` baseline of 1.24 Wh/call — a 4.4x per-call reduction matching the 300 W / 80 W coefficient ratio.

### Security
- Removed machine-specific `src-tauri/.cargo/config.toml` from the repo (was leaking an absolute Windows rustc path). History scrubbed via `git filter-repo` and force-pushed.
- `.gitignore` now covers `src-tauri/.cargo/` and `.claude/` so local machine-specific config files can never be tracked again.
- Local `.git/hooks/pre-commit` scans every staged commit for absolute home-directory paths and refuses the commit if any are found.

## [0.2.0] - 2026-04-17

### Added — Dual-Brain Intelligence Architecture
- **6-Layer Context Bundle** (`context_bundle.rs`): Rules, knowledge (MMR-selected), work patterns, decisions, warnings, predictions — compressed to ~4000 tokens per inject cycle
- **MMR diversity selection**: Maximal Marginal Relevance ensures context bundles contain diverse, non-redundant information
- **5 new MCP learning tools**: `brain_warnings`, `brain_rules`, `brain_learn_decision`, `brain_learn_pattern`, `brain_learn_mistake` — Claude can now teach the brain back
- **Cross-session continuity** (`session_continuity.rs`): Extracts decisions/code/problems/questions/next-steps from completed sessions and writes handoff files
- **Context quality tracking** (`context_quality.rs`): Logs bundle effectiveness, detects knowledge gaps, spawns research missions for unfilled gaps
- **Anticipatory loading** (`anticipatory.rs`): Predicts next context based on active project, time of day, and pending research missions
- **Dream mode** (`dream_mode.rs`): Overnight 5-pass deep synthesis (22:00-06:00) and morning briefings (05:00-08:00) compiled before you wake up
- **5 new circuits**: `session_summarizer`, `context_quality_optimizer`, `anticipatory_preloader`, `deep_synthesis`, `morning_briefing` (36 total, up from 31)
- **7 new HTTP endpoints**: `/brain/bundle`, `/brain/warnings`, `/brain/rules`, `/brain/learn_decision`, `/brain/learn_pattern`, `/brain/learn_mistake`, `/brain/session_handoff`

### Fixed
- Master loop SQL column mismatch (`command` → `tool_name`)
- Chat sync watcher now auto-detects `~/.claude/projects` + `~/.cursor/chats` (was hardcoded to `~/.ai-assistant/projects`)
- Missing `public/` directory (brain-model.glb, brain-icon.svg) restored
- Missing `tailwind.config.ts` restored
- Rust MSVC toolchain pin via `.cargo/config.toml`

### Improved
- `.gitignore` hardened with 20+ patterns to prevent data leakage
- CI/release workflows now use `swatinem/rust-cache@v2` for faster builds
- README: added brain screenshot, documentation links, fixed URLs

## [0.1.0] - 2026-04-14

### Added
- Initial open-source release
- SQLite database with FTS5 full-text search and WAL mode
- 31 autonomous self-improvement circuits
- 3D brain visualization with Three.js
- HNSW vector search for semantic similarity
- Obsidian-compatible vault (plain markdown files)
- PDF and DOCX file ingestion
- RSS/API data stream ingestion
- Phase Omega systems:
  - Digital Twin (cognitive fingerprint, decision simulator, internal dialogue)
  - Agent Swarm (5 specialist agents with goal decomposition)
  - World Model (causal entities, temporal patterns, predictions)
  - Self-Improvement (knowledge compiler, circuit optimizer, capability tracker)
  - Sensory Expansion (visual analysis, audio transcription, data streams)
  - Federation Protocol (multi-brain knowledge sharing)
  - Infrastructure (distributed architecture, edge caching, system health)
  - Economic Autonomy (revenue/cost tracking, ROI calculation)
  - Consciousness Layer (self-model, attention mechanism, curiosity v2)
- MCP server for AI assistant integration
- Onboarding wizard for first-run experience
- Security hardening (SQL injection prevention, CORS, rate limiting)

### Security
- SQL injection prevention with table name allowlists
- CORS restricted to localhost origins
- Rate limiting (300 req/min)
- Input validation on all endpoints
- Path traversal protection
- Command injection prevention
