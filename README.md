# NeuroVault

**A self-evolving 3D knowledge brain that runs entirely on your machine.**

No cloud. No accounts. No telemetry.

NeuroVault is a desktop knowledge management system that ingests your notes, code, conversations, and documents, then organizes them into a living 3D knowledge graph that grows smarter over time through autonomous circuits.

---

## Features

- **3D Knowledge Graph** -- Interactive Three.js visualization of your entire knowledge base. Fly through your brain, click nodes, explore connections.
- **31 Autonomous Circuits** -- Background processes that continuously improve your knowledge: cross-domain fusion, quality scoring, gap detection, pattern mining, curiosity-driven research, and more.
- **IQ Scoring** -- A composite intelligence metric that tracks how connected, deep, and useful your knowledge is. Watch it climb as the brain self-improves.
- **Semantic Search** -- HNSW vector index + FTS5 full-text search. Find anything instantly by meaning, not just keywords.
- **MCP Integration** -- Ships with an MCP server that bridges Claude Code directly to your brain. Your AI coding assistant remembers everything.
- **Phase Omega Systems**:
  - **Digital Twin** -- A self-model that tracks the brain's own strengths, weaknesses, and improvement priorities.
  - **Agent Swarm** -- Spawn autonomous research agents that investigate topics and report back.
  - **World Model** -- Tracks entities, relationships, and predictions about the external world.
  - **Self-Improvement** -- The brain identifies its own gaps and autonomously fills them.
  - **Economic Autonomy** -- Tracks the value the brain generates vs. its compute costs.
  - **Multi-Brain Federation** -- Share knowledge between NeuroVault instances.
  - **Sensory Expansion** -- Ingest images, audio, screenshots, and live data streams.
- **Obsidian-Style Vault** -- Every node is also a plain markdown file with YAML frontmatter. Your data is always yours.
- **Fine-Tuning Pipeline** -- Export your knowledge as training data and fine-tune a local LLM that thinks like you.
- **Proactive Sidekick** -- Watches your Claude Code sessions and injects relevant context automatically.

## Quick Start

### Download

Pre-built binaries for Windows, macOS, and Linux are available on the [Releases](https://github.com/neurovault/neurovault/releases) page.

### Build from Source

**Prerequisites:**
- [Node.js](https://nodejs.org/) >= 18
- [pnpm](https://pnpm.io/)
- [Rust](https://rustup.rs/) (latest stable)
- [Ollama](https://ollama.ai/) (for local LLM and embeddings)

```bash
# Clone the repo
git clone https://github.com/neurovault/neurovault.git
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

### Docker (Headless Mode)

```bash
docker run -d \
  -p 17777:17777 \
  -v ~/.neurovault:/root/.neurovault \
  neurovault/neurovault:latest
```

The headless binary runs the autonomy engine + HTTP API without the desktop GUI, ideal for servers.

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Desktop Shell | Tauri v2 |
| Frontend | React 19 + Three.js + Zustand |
| Backend | Rust (Tokio async runtime) |
| Database | SQLite + FTS5 (bundled, zero-config) |
| Vector Search | HNSW (instant-distance, pure Rust) |
| LLM / Embeddings | Ollama (local) or Anthropic API |
| MCP Bridge | Node.js stdio server |

## Architecture

```
~/.neurovault/
  data/brain.db          # SQLite database (WAL mode)
  data/hnsw.bin          # HNSW vector index
  vault/                 # Markdown files (Obsidian-compatible)
  export/                # Auto-generated exports for Claude Code
  finetune/              # Training scripts and datasets
  backups/               # Automated backups
```

## MCP Server

NeuroVault ships with an MCP server that lets Claude Code query your brain directly:

```json
{
  "mcpServers": {
    "neurovault": {
      "command": "node",
      "args": ["/path/to/neurovault/mcp-server/src/index.js"]
    }
  }
}
```

Available tools: `brain_recall`, `brain_ingest`, `brain_preferences`, `brain_decisions`, `brain_learn`, `brain_health`, `brain_stats`, `brain_critique`, `brain_subgraph`, `brain_plan`.

## Community Circuits

NeuroVault's circuit system is extensible. Community-contributed circuits can add new autonomous behaviors to your brain:

- **Circuit format**: Each circuit is a Rust module implementing the circuit trait
- **Installation**: Drop circuit files into `src-tauri/src/circuits/` and register them
- **Sharing**: Publish circuits as standalone crates or submit PRs to this repo

See [CONTRIBUTING.md](CONTRIBUTING.md) for details on writing custom circuits.

## Contributing

Contributions are welcome! Whether it's bug fixes, new circuits, UI improvements, or documentation:

1. Fork the repo
2. Create a feature branch (`git checkout -b feature/my-circuit`)
3. Commit your changes
4. Push to your fork and open a Pull Request

Please see [CONTRIBUTING.md](CONTRIBUTING.md) for coding standards and architecture guidelines.

## License

[MIT](LICENSE)
