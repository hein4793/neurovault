# NeuroVault — Dual-Brain Intelligence Architecture

> Local Qwen handles the 24/7 grunt work. Claude Opus handles the thinking.
> The brain makes both of them smarter than either could be alone.

## Implementation Status

| Phase | Name | Status | Files |
|-------|------|--------|-------|
| 1 | Enhanced Context Bundle | DONE | `context_bundle.rs`, updated `sidekick.rs`, `/brain/bundle` endpoint |
| 2 | MCP Learning Tools | DONE | `mcp-server/src/index.js` (+5 tools), `http_api.rs` (+6 endpoints) |
| 3 | Cross-Session Continuity | DONE | `session_continuity.rs`, `session_summarizer` circuit, `/brain/session_handoff` |
| 4 | Context Quality + Gap Detection | DONE | `context_quality.rs`, `context_quality_optimizer` circuit, quality logging in sidekick |
| 5 | Anticipatory Loading | DONE | `anticipatory.rs`, `anticipatory_preloader` circuit, time/project/mission prediction |
| 6 | Dream Mode | DONE | `dream_mode.rs`, `deep_synthesis` + `morning_briefing` circuits, overnight window gating |

## Phase 1: Enhanced Context Bundle (DONE)

6-layer intelligence package injected into every Claude Code session:

1. **Compiled Rules** — deterministic if/then/always/never rules from `knowledge_rules` table
2. **Knowledge Nodes** — HNSW + FTS5 search with MMR diversity selection
3. **Work Patterns** — user_cognition behavioral patterns ranked by relevance
4. **Decision Memory** — past decisions on the current topic
5. **Warnings** — past mistakes, contradictions, known pitfalls
6. **Predictions** — what the user will likely need next

Token budget: ~4000 tokens total, allocated per section. MMR ensures diversity.
Output: structured `sidekick-context.md` + `/brain/bundle` HTTP endpoint.

## Phase 2: MCP Learning Tools

5 new MCP tools so Claude can teach the brain back:
- `brain_warnings` — query past mistakes/contradictions
- `brain_rules` — query compiled deterministic rules
- `brain_learn_decision` — record a decision with reasoning
- `brain_learn_pattern` — record a behavioral pattern
- `brain_learn_mistake` — record a mistake/warning

## Phase 3: Cross-Session Continuity

- Session summarizer circuit (extract decisions/patterns/code from completed sessions)
- Session handoff protocol (inject last session summary into new sessions)
- Session summaries stored as nodes with `node_type='session_summary'`

## Phase 4: Context Quality + Gap Detection

- Track which injected context Claude actually uses vs. ignores
- Detect when Claude asks for info the brain has but didn't inject
- `context_quality_optimizer` circuit to tune relevance weights

## Phase 5: Anticipatory Loading

- Project-based preloading (architecture + recent decisions)
- Time-based preloading (morning briefing, post-lunch deep work)
- Sequence-based preloading (after test file → testing patterns)
- Query complexity detection for multi-granularity context

## Phase 6: Dream Mode

- `deep_synthesis` — 5-pass reasoning on day's top nodes (overnight)
- `cross_domain_dreams` — force random connections for novel insights
- `prediction_generation` — generate predictions for next day
- `morning_briefing` — compile overnight discoveries
- Idle detection to trigger overnight processing

## Architecture

```
HEIN (interactive) ↔ CLAUDE OPUS (via Claude Code + MCP)
                         ↕
                    MCP SIDEKICK BRIDGE
                    ↕               ↕
              CONTEXT OUT      LEARNING IN
                    ↕               ↕
              NEUROVAULT BRAIN (222K+ nodes)
                         ↕
                   LOCAL QWEN 14B (72 circuits/day)
```

## Key Insight

You don't need to fine-tune when you can inject context perfectly at runtime.
Runtime injection is always current; fine-tuned weights are frozen at training time.
When Anthropic releases a new model, the brain works with it immediately.
