# Troubleshooting

## Ollama Issues

### Ollama not detected
- Ensure Ollama is running: `ollama serve`
- Check if accessible: `curl http://localhost:11434/api/tags`
- Default port is 11434. If using a different port, set `NEUROVAULT_HTTP_PORT`

### Model pull fails
- Check internet connection
- Try manually: `ollama pull nomic-embed-text`
- Check disk space — models need 1-8GB each

### Embedding pipeline stuck at 0
- Verify nomic-embed-text is installed: `ollama list`
- Check the settings.json: `~/.neurovault/data/settings.json`
- Restart the app — embeddings process 50 nodes every 30 seconds

## Build Issues

### Windows
- Install Visual Studio Build Tools (C++ workload)
- Install WebView2 runtime
- Run `rustup default stable-x86_64-pc-windows-msvc`

### macOS
- Install Xcode command line tools: `xcode-select --install`
- For Apple Silicon: `rustup target add aarch64-apple-darwin`

### Linux
- Install system deps: `sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev`

## Performance

### Brain is slow with 100K+ nodes
- Quality scoring runs 10K nodes per cycle — this is normal
- FTS5 search is instant at any scale
- HNSW vector search rebuilds every 200 new embeddings
- Close unnecessary Ollama models: `ollama stop <model>`

### High memory usage
- Each Ollama model uses 2-16GB VRAM
- The brain itself uses ~200MB RAM for 200K nodes
- Reduce `LIMIT` values in config for lower memory

### Disk space
- SQLite: ~8 bytes per node (200K nodes ≈ 1.6GB)
- HNSW index: ~150MB for 50K embeddings
- Vault: ~1KB per markdown file
- Weekly exports can be disabled in settings

## Common Errors

### "Database is locked"
- Only one instance should run at a time
- Kill any stale processes: `taskkill /F /IM neurovault.exe` (Windows)

### "FTS5 not available"
- The bundled SQLite includes FTS5 — if you see this, rebuild from source
- Run `cargo clean && cargo build`

### Circuit failures
- Circuits retry automatically on the next rotation (every 20 minutes)
- Check the Brain Activity panel for error details
- Ensure Ollama is running with the configured model
