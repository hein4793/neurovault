# Installation Guide

NeuroVault runs on Windows, macOS, and Linux. All data stays on your machine.

---

## 1. Install prerequisites

| Tool | Purpose | Version |
|------|---------|---------|
| [Rust](https://rustup.rs/) | Compiles the backend | Latest stable |
| [Node.js](https://nodejs.org/) | Runs the dev toolchain and MCP server | >= 18 |
| [pnpm](https://pnpm.io/installation) | Frontend package manager | >= 9 |
| [Ollama](https://ollama.ai/) | Local LLM and embeddings | Latest |

### Platform-specific system deps

**Windows 10/11** — WebView2 is usually pre-installed. Nothing else needed.

**macOS** — install Xcode Command Line Tools:
```bash
xcode-select --install
```

**Linux (Debian/Ubuntu)**:
```bash
sudo apt install -y \
  libwebkit2gtk-4.1-dev \
  libgtk-3-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev \
  build-essential
```

**Linux (Fedora)**:
```bash
sudo dnf install -y webkit2gtk4.1-devel gtk3-devel libappindicator-gtk3-devel librsvg2-devel
```

**Linux (Arch)**:
```bash
sudo pacman -S webkit2gtk-4.1 gtk3 libappindicator-gtk3 librsvg
```

---

## 2. Pull the Ollama models

NeuroVault needs an embedding model at minimum. A generation model enables LLM-powered circuits (synthesis, debate, self-reflection).

```bash
# Required: embeddings (smaller, fast)
ollama pull nomic-embed-text

# Recommended: fast generation model
ollama pull qwen2.5-coder:14b

# Optional: larger model for deeper reasoning
ollama pull qwen2.5-coder:32b

# Optional: vision model for screenshot/image ingestion
ollama pull moondream
```

Verify:
```bash
ollama list
```

Start the Ollama daemon if it isn't already:
```bash
ollama serve
```

---

## 3. Clone and install NeuroVault

```bash
git clone https://github.com/hein4793/neurovault.git
cd neurovault
pnpm install
```

---

## 4. First run (development mode)

```bash
pnpm tauri dev
```

The first build compiles the Rust backend from scratch — expect 3-8 minutes depending on your machine. Subsequent launches are near-instant.

Once running, you'll see:
- Desktop window with the 3D knowledge graph
- HTTP API listening on `http://127.0.0.1:17777`
- Data directory created at `~/.neurovault/`
- One circuit firing every 20 minutes (autonomy loop)

Health check:
```bash
curl http://127.0.0.1:17777/health
# {"service":"neurovault","status":"ok","version":"0.1.0"}
```

---

## 5. Production build (optional)

```bash
pnpm tauri build
```

Bundles a native installer into `src-tauri/target/release/bundle/`:
- Windows: `.msi` and `.exe`
- macOS: `.dmg` and `.app`
- Linux: `.deb`, `.AppImage`, `.rpm`

---

## 6. Data directory

All personal data lives at `~/.neurovault/` — outside the repo, never touched by git:

```
~/.neurovault/
  data/
    brain.db           # SQLite (WAL mode)
    brain.db-wal       # Write-ahead log
    hnsw.bin           # Vector index
  vault/               # Your knowledge as markdown (Obsidian-compatible)
  export/              # Auto-generated briefings for your AI assistant
  backups/             # Automated SQLite backups
  logs/                # Per-session logs
  settings.json        # User preferences
```

Back up the whole directory to preserve your brain. No cloud sync unless you wire one up yourself.

---

## 7. MCP wiring (Claude Code, Cursor, etc.)

See [MCP_INTEGRATION.md](MCP_INTEGRATION.md) for hooking NeuroVault into your AI coding assistant.

---

## Troubleshooting

**`tauri is not recognized`** — run `pnpm install` from the repo root, then retry.

**Cargo compile fails on Windows** — ensure "Desktop development with C++" is installed via Visual Studio Build Tools.

**Ollama 404 on embeddings** — `ollama pull nomic-embed-text` wasn't run or the daemon isn't started. Check `ollama list` and `ollama serve`.

**Port 17777 in use** — another instance is running. Stop it, or set `NEUROVAULT_HTTP_PORT=17778` in your environment.

**WebView error on Linux** — your `libwebkit2gtk` is too old or mis-versioned. The Tauri v2 requirement is `libwebkit2gtk-4.1`, not 4.0.

For more, see [TROUBLESHOOTING.md](TROUBLESHOOTING.md).
