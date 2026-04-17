# Community Announcement Drafts

Private drafts for launching NeuroVault on community channels. **Do not post automatically** — review tone, verify links, and tailor to each venue before publishing.

---

## r/LocalLLaMA

**Title**: I built a self-evolving 3D knowledge brain that runs entirely on local LLMs — open source

**Body**:

NeuroVault is a desktop knowledge system that ingests your notes, code, and chats, organizes them into a 3D graph, and **keeps improving on its own** via 31 autonomous circuits running in the background.

It's fully local. All embeddings and generation run through Ollama. No cloud, no accounts, no telemetry.

**What makes it different:**
- 31 self-improvement circuits fire on a 20-min rotation → the brain literally grows overnight
- IQ score tracks how connected and useful your knowledge is
- Cognitive fingerprint — a self-model of your thinking patterns that can simulate decisions
- Agent swarm for autonomous research
- Obsidian-style vault: every node is also a plain markdown file
- MCP server so your AI coding assistant queries the brain directly

**Stack**: Tauri v2 + React 19 + Three.js + SQLite (FTS5 + WAL) + HNSW (pure Rust) + Ollama

Tested with `qwen2.5-coder:14b` for fast generation and `nomic-embed-text` for embeddings.

GitHub: https://github.com/hein4793/neurovault

Happy to answer questions about the architecture, circuit system, or how the self-improvement loop actually works.

---

## r/Ollama

**Title**: NeuroVault — a 3D knowledge brain that uses Ollama for embeddings + generation, fully open source

**Body**:

Wanted to share a project I've been building that leans heavily on Ollama.

NeuroVault ingests your notes, code, PDFs, chat logs, RSS feeds, and screenshots, then organizes everything into a self-improving knowledge graph. Every 20 minutes a background "circuit" fires — synthesizing insights, detecting contradictions, filling knowledge gaps, running internal debates, etc.

**Ollama integration:**
- Embeddings via `nomic-embed-text` (768-dim)
- Generation via `qwen2.5-coder:14b` (fast) or `qwen2.5-coder:32b` (deeper)
- Vision via `moondream` or `llava` for screenshot analysis
- Optional Anthropic API fallback for heavy reasoning

**Why I'm posting here:** if you're running Ollama, NeuroVault gives you something to do with it beyond chat — a 24/7 knowledge system that gets smarter while you sleep.

MIT licensed. Tauri desktop app for Win/Mac/Linux.

GitHub: https://github.com/hein4793/neurovault

Feedback welcome, especially on circuit design and embedding strategies.

---

## Hacker News (Show HN)

**Title**: Show HN: NeuroVault – a self-improving 3D knowledge brain that runs locally

**Body**:

I wanted a knowledge system that would actually *think about my notes* while I wasn't looking — not just sit there waiting for me to search it.

NeuroVault is the result. It's a Tauri desktop app that ingests your notes, code, chats, PDFs, and RSS feeds into a SQLite + HNSW-backed knowledge graph, visualized as a 3D force-directed graph. A rotating dispatcher fires one of 31 autonomous "circuits" every 20 minutes:

- `cross_domain_fusion` — bridges concepts from different domains
- `contradiction_detector` — finds conflicting beliefs
- `internal_dialogue` — runs advocate/critic debates
- `curiosity_gap_fill` — researches topics the brain flagged as under-represented
- `self_reflection` — maintains a model of the brain's own strengths and weaknesses
- `economic_audit` — tracks value generated vs. compute cost

72 improvement cycles per day. The brain gets smarter while you sleep.

Stack: Rust (Tokio) backend, React 19 + Three.js frontend, SQLite (FTS5/WAL) for knowledge, HNSW (pure Rust `instant-distance`) for vector search, Ollama for local LLM + embeddings.

Ships with an MCP server so Claude Code / Cursor / other AI assistants can query the brain directly during coding.

Fully local, no telemetry, no accounts. Data lives in `~/.neurovault/` as SQLite + Obsidian-compatible markdown.

MIT licensed. Happy to answer anything about the architecture or the circuit system — the self-improvement loop is the most interesting part.

https://github.com/hein4793/neurovault

---

## Tauri Discord (#showcase)

Hey all 👋

Dropping this in case anyone wants a reference for a non-trivial Tauri v2 app:

**NeuroVault** — a 3D knowledge brain that runs entirely locally, now open source.

- Tauri v2 + React 19 + Three.js + Zustand (state)
- Rust backend with 31 autonomous "circuits" firing on a rotation
- SQLite (FTS5 + WAL) + HNSW pure-Rust vector index
- Axum HTTP API on 127.0.0.1:17777 for external MCP access
- Obsidian-compatible markdown vault on disk

Things I learned along the way that might save you time:
- `tauri-plugin-fs` version has to match exactly between NPM and Rust crate (it screams at you otherwise)
- Bundled SQLite with `rusqlite { features = ["bundled", "vtab", "modern_sqlite"] }` just works
- `instant-distance` for HNSW is great if you don't want ONNX or Python deps
- Axum + Tauri IPC side-by-side is clean — desktop gets IPC, external tools get HTTP

Repo: https://github.com/hein4793/neurovault

Would love feedback from anyone who's shipped a production Tauri v2 app — especially on bundling and auto-update flows.

---

## Twitter / X thread

**Tweet 1/**
I open-sourced NeuroVault.

A 3D knowledge brain that runs entirely on your machine and *keeps improving while you sleep*.

🧵

**Tweet 2/**
31 autonomous circuits fire in rotation — one every 20 minutes.

They synthesize insights, detect contradictions, fill knowledge gaps, run internal debates, track a cognitive fingerprint, audit themselves…

72 improvement cycles a day. The brain actually gets smarter overnight.

**Tweet 3/**
Fully local. Ollama handles embeddings + generation. SQLite + FTS5 + HNSW for storage and search.

No cloud. No accounts. No telemetry.

Tauri v2 desktop app for Win/Mac/Linux. MIT license.

**Tweet 4/**
Ships with an MCP server so Claude Code, Cursor, and other AI assistants can query the brain directly while you code.

Every node is also a plain markdown file, Obsidian-compatible.

**Tweet 5/**
https://github.com/hein4793/neurovault

Would love feedback — especially on the circuit system. This is the part I'm most proud of.

---

## LinkedIn (professional tone)

**Title**: Announcing NeuroVault — open-source self-improving knowledge graph

**Body**:

I've been building a personal knowledge system that doesn't just store information — it reasons about it, finds gaps, and fills them autonomously. Today I'm open-sourcing it.

**NeuroVault** is a Tauri desktop application that:
- Ingests notes, code, conversations, PDFs, and live data streams
- Organizes everything into a 3D knowledge graph (SQLite + HNSW vector index)
- Runs 31 autonomous circuits on a 20-minute rotation to continuously improve the graph
- Maintains a self-model of its own strengths, weaknesses, and improvement priorities
- Exposes the brain to AI coding assistants via an MCP server

All processing is local (via Ollama). No cloud dependency, no telemetry, no account required.

**Technical highlights:**
- Rust backend (Tokio async runtime)
- React 19 + Three.js for 3D visualization
- SQLite with FTS5 and WAL mode — scales to millions of nodes
- HNSW vector search (pure Rust, no Python deps)
- MIT licensed

The circuit architecture is what I'm most proud of — each circuit is a small, idempotent async function that contributes one specific kind of cognitive work (synthesis, contradiction detection, cross-domain bridging, etc.). The system composes them into autonomous self-improvement.

Feedback, issues, and contributions welcome.

GitHub: https://github.com/hein4793/neurovault

#OpenSource #Rust #KnowledgeManagement #LocalLLM #Tauri
