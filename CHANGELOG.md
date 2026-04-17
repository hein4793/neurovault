# Changelog

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
