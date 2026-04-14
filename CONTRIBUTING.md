# Contributing to NeuroVault

Thank you for your interest in contributing to NeuroVault! Whether it's bug fixes, new circuits, UI improvements, or documentation, all contributions are welcome.

## How to Contribute

### Reporting Bugs

1. Check existing [issues](https://github.com/hein4793/neurovault/issues) to avoid duplicates
2. Open a new issue with:
   - Clear title and description
   - Steps to reproduce
   - Expected vs. actual behavior
   - OS, Rust version, Node version

### Suggesting Features

Open an issue with the `enhancement` label. Describe the use case and how it fits into the existing architecture.

### Submitting Pull Requests

1. Fork the repository
2. Create a feature branch: `git checkout -b feature/my-feature`
3. Make your changes
4. Run checks (see below)
5. Commit with a clear message
6. Push to your fork and open a PR against `master`

### Contributing Circuits

Circuits are the heart of NeuroVault's self-improvement engine. See the [Circuit Architecture](#circuit-architecture) section below and the [Circuit Guide](docs/CIRCUIT_GUIDE.md) for details.

---

## Development Setup

### Prerequisites

- [Node.js](https://nodejs.org/) >= 18
- [pnpm](https://pnpm.io/)
- [Rust](https://rustup.rs/) (latest stable)
- [Ollama](https://ollama.ai/) (for local LLM and embeddings)
- Platform-specific dependencies:
  - **Linux**: `libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev`
  - **macOS**: Xcode Command Line Tools
  - **Windows**: WebView2 (usually pre-installed on Windows 10/11)

### Build from Source

```bash
# Clone your fork
git clone https://github.com/YOUR_USERNAME/neurovault.git
cd neurovault

# Install frontend dependencies
pnpm install

# Pull the embedding model
ollama pull nomic-embed-text

# Run in development mode
pnpm tauri dev

# Or build for production
pnpm tauri build
```

---

## Code Style

### Rust

- Run `cargo fmt` before committing
- Run `cargo clippy` and address warnings
- Use `cargo test` to verify nothing is broken

```bash
cd src-tauri
cargo fmt
cargo clippy
cargo test
```

### Frontend (TypeScript/React)

- Run `pnpm lint` before committing
- Follow existing patterns in the `src/` directory

```bash
pnpm lint
pnpm build
```

---

## Circuit Architecture

NeuroVault runs **31 autonomous circuits** on a 20-minute rotation. Each cycle, the dispatcher picks the next circuit that hasn't run in the last 3 cycles, executes it, and logs the result to `autonomy_circuit_log`.

### Circuit Categories

| Phase | Circuits | Purpose |
|-------|----------|---------|
| Phase 0 | meta_reflection, user_pattern_mining, cross_domain_fusion, quality_recalc, self_synthesis, curiosity_gap_fill, iq_boost | Core self-improvement: reflection, pattern mining, bridging, quality, synthesis, research, IQ |
| Phase 1 | compression_cycle | Memory compression and consolidation |
| Phase 2 | contradiction_detector, decision_memory_extractor, knowledge_synthesizer, self_assessment, prediction_validator, hypothesis_tester, code_pattern_extractor | Cognitive capabilities: reasoning, decisions, synthesis, testing |
| Phase 4 | synapse_prune | Graph pruning and optimization |
| Omega | fingerprint_synthesis, internal_dialogue | Digital twin: cognitive fingerprint, inner monologue |
| Omega II | swarm_orchestrator | Agent swarm task management |
| Omega III | temporal_analysis, causal_model_builder, scenario_simulator | World model: temporal patterns, causality, simulation |
| Omega IV | knowledge_compiler, circuit_optimizer, capability_tracker | Recursive self-improvement |
| Omega IX | self_reflection, attention_update, curiosity_v2 | Consciousness layer |
| Omega VI | federation_sync | Multi-brain federation |
| Omega VII | cluster_health_check | Infrastructure monitoring |
| Omega VIII | economic_audit | Economic autonomy tracking |

### How to Write a New Circuit

1. **Add the circuit name** to the `ALL_CIRCUITS` array in `src-tauri/src/circuits.rs`
2. **Add a match arm** in the `run_circuit` function in `circuits.rs`
3. **Implement the logic** as an async function that takes `&Arc<BrainDb>` and returns `Result<String, BrainError>`
4. **Log what you did** -- return a summary string on success

#### Circuit Template

```rust
// In src-tauri/src/circuits.rs, add to run_circuit():
"my_new_circuit" => my_new_circuit(db).await,

// Then implement:
async fn my_new_circuit(db: &Arc<BrainDb>) -> Result<String, BrainError> {
    // 1. Query the database for relevant data
    let items = db.with_conn(|conn| {
        // ... your query here
        Ok(vec![])
    }).await?;

    if items.is_empty() {
        return Ok("No items to process".to_string());
    }

    // 2. Process/analyze the data
    let mut processed = 0;
    for item in &items {
        // ... your processing logic
        processed += 1;
    }

    // 3. Write results back (new nodes, updated scores, etc.)
    // ...

    // 4. Return a summary
    Ok(format!("Processed {} items", processed))
}
```

#### Circuit Guidelines

- Keep execution time under 5 minutes
- Be idempotent where possible -- running twice shouldn't cause problems
- Use `db.with_conn()` for all database access
- Handle errors gracefully -- a failing circuit shouldn't crash the rotation
- Return a descriptive result string for the circuit log

---

## PR Process

1. **Fork** the repository
2. **Branch** from `master`: `git checkout -b feature/my-feature`
3. **Code** your changes following the style guidelines
4. **Test** locally: `cargo test`, `cargo clippy`, `pnpm build`
5. **Commit** with a clear, descriptive message
6. **Push** to your fork
7. **Open a PR** against `master` with:
   - Description of what changed and why
   - Screenshots for UI changes
   - Which circuits are affected (if any)
8. **Address review feedback**

### Circuit Contribution Checklist

When submitting a new circuit:

- [ ] Circuit name added to `ALL_CIRCUITS` in `circuits.rs`
- [ ] Match arm added in `run_circuit()`
- [ ] Implementation follows the template pattern
- [ ] Execution time stays under 5 minutes
- [ ] Error handling returns `BrainError`, doesn't panic
- [ ] Result string is descriptive
- [ ] Brief doc comment explaining the circuit's purpose
