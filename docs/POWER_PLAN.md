# Power Plan — Latency-Stratified Inference Routing

NeuroVault's 36 circuits run around the clock. Sending every inference call to the GPU means a modern desktop (12-core CPU + 16 GB-class GPU) can easily draw 600-850 W sustained during heavy background work like `self_synthesis` or `master_loop`. That's expensive, noisy, and fatal to any UPS.

The Power Plan solves this by stratifying circuits by latency tolerance: interactive calls (where the user is waiting) stay on the GPU; 24/7 batch circuits route to a second Ollama daemon running CPU-only at roughly a quarter the draw. An adaptive policy layer on top automatically demotes everything to CPU when the machine is on battery.

## How it fits together

```
  +---------------------------------------------------------+
  |                  circuits::run_circuit                  |
  |    (tokio::task_local! CURRENT_CIRCUIT = <name>)        |
  +--------------------------+------------------------------+
                             |
                             v
              +--------------+---------------+
              |  commands::ai factory        |
              |  (reads profile + policy)    |
              +--------------+---------------+
                             |
             +---------------+---------------+
             |                               |
             v                               v
     +---------------+               +---------------+
     | ollama-vulkan |               |  ollama-cpu   |
     |  (GPU, 300 W) |               |  (~80 W)      |
     +-------+-------+               +-------+-------+
             |                               |
             +---------------+---------------+
                             |
                             v
                +------------+-------------+
                |  power_telemetry writes  |
                |  one row to inference_log |
                +--------------------------+
```

- `power_telemetry.rs` owns the `inference_log` table, the `CircuitProfile` enum, the energy-coefficient table, and the rollup query.
- `power_policy.rs` owns the `PowerMode` atomic + the 30 s AC-line poll loop.
- `ai/client.rs` hooks `LlmClient::generate_ollama` and `generate_anthropic` to record every completed call.
- `commands/ai.rs` factory consults both `current_profile()` and `prefer_cpu()` to pick the backend on every construction.

## The six phases

### Phase 1 — Telemetry

**What:** Every LLM call gets a row in a new `inference_log` table with `circuit`, `backend`, `model`, token counts, `duration_ms`, and `energy_wh`.

**How:** `tokio::task_local! CURRENT_CIRCUIT` is set by `run_circuit()` so the 36 individual circuit functions don't need to thread a name through. `LlmClient` spawns a detached recorder on every successful `generate`; telemetry failures are logged but never propagated.

**Test:** `GET /metrics/power?hours=24` returns a populated rollup after any call completes.

### Phase 2 — CPU backend infrastructure

**What:** Add optional `ollama_cpu_url` to `BrainConfig` (overridable via `NEUROVAULT_OLLAMA_CPU_URL`) and a `backend_tag` field on `LlmClient` so the telemetry correctly tags `ollama-cpu` rows.

**How:** New factory variants `get_llm_client_cpu` / `get_llm_client_cpu_fast` route explicitly. When `ollama_cpu_url` is unset they fall back to the GPU pool so callers never break on single-daemon machines.

**Test:** With both daemons running, a call made through `get_llm_client_cpu` appears in `/metrics/power` under `backend = "ollama-cpu"`.

### Phase 3 — Circuit profiles and automatic routing

**What:** Each circuit has a `CircuitProfile` -- `Interactive` (user is waiting), `NearRealTime` (user might glance but isn't blocked), or `Batch` (pure background). The factory auto-routes Batch circuits to CPU.

**How:** The profile table in `power_telemetry::circuit_profile` maps circuit names explicitly. Any scoped name not in the list defaults to `NearRealTime`; the unscoped fallback `"unknown"` maps to `Interactive` so ad-hoc user-initiated calls (e.g. from HTTP handlers) never accidentally slow down.

**Test:** Trigger a Batch circuit via the rotation and verify its next LLM call shows `backend = "ollama-cpu"` in the rollup.

### Phase 4 — Adaptive power policy

**What:** A `PowerMode` state machine: `Normal` / `Eco` / `IdleOpportunistic` / `ThermalThrottle` / `LoadShed`. In `Eco`, every call demotes to CPU regardless of profile. In `LoadShed`, batch circuits are queued instead of run.

**How:** `power_policy::run_power_policy_loop` polls every 30 s. AC detection on Windows uses a direct `kernel32::GetSystemPowerStatus` FFI call -- no new crate deps. On non-Windows builds the detector returns `None` and the policy stays in `Normal`.

**Test:** Unplug the AC; within 30 s `GET /metrics/power/status` returns `mode: "eco"` and `prefer_cpu: true`.

### Phase 5 — Model tiering

**What:** CPU paths use smaller models than GPU paths. A 14 B model on CPU is ~1-3 tok/s which defeats the purpose.

**How:** The factory reads a new `llm_model_cpu` setting when constructing a CPU client; falls back to `llm_model_fast` and finally to a conservative default (`qwen2.5:3b`, ~2 GB, typically 10-15 tok/s on recent consumer CPUs with 64 GB RAM).

**Test:** With `llm_model_cpu` unset, CPU-routed calls in `inference_log` should show `model = "qwen2.5:3b"`.

### Phase 6 — Dashboard surface

**What:** Two HTTP endpoints surface live state for any frontend or operator script.

- `GET /metrics/power?hours=N` -- rollup with `total_energy_wh`, `avg_watts`, and `annualized_kwh` projected from the measured window.
- `GET /metrics/power/status` -- live `mode`, `prefer_cpu`, `on_battery`, `cpu_daemon_configured`, and the per-backend wattage coefficients.

**How:** Both handlers live in `http_api.rs` alongside the existing `/health` and `/stats`. See [`API_REFERENCE.md`](API_REFERENCE.md) for request/response shapes.

## Activation checklist

Phases 1-6 are all compiled into the main binary. To activate the CPU routing on your machine:

**1. Start a second Ollama daemon, CPU-only, on a non-default port:**

```bash
# Unix shells
OLLAMA_HOST=127.0.0.1:11435 OLLAMA_NUM_GPU=0 ollama serve
```

```powershell
# PowerShell
$env:OLLAMA_HOST="127.0.0.1:11435"; $env:OLLAMA_NUM_GPU="0"; ollama serve
```

**2. Pull a small model to that daemon:**

```bash
OLLAMA_HOST=127.0.0.1:11435 ollama pull qwen2.5:3b
```

Ollama shares its model store across daemons, so this only downloads once even if you already have other models locally.

**3. Launch the brain with the CPU URL set:**

```bash
NEUROVAULT_OLLAMA_CPU_URL=http://127.0.0.1:11435 pnpm tauri dev
```

To make the setting permanent: add `NEUROVAULT_OLLAMA_CPU_URL` to your shell profile or your OS's user environment variables.

**4. Verify:**

```bash
curl -s http://127.0.0.1:17777/metrics/power/status
```

`cpu_daemon_configured` should be `true`. Once a batch circuit fires (every ~20 minutes during normal operation, or when you trigger one manually), `GET /metrics/power?hours=1` will show a non-zero `ollama-cpu` entry under `by_backend`.

## Expected impact

Rough math on a 24 h window for a reference workstation (i7-12700F + RX 6900 XT, 64 GB, Windows 11):

| State                                         | Before  | After   |
|-----------------------------------------------|---------|---------|
| Idle + sidekick only                          | 300 W   | 280 W   |
| Background circuits firing on 14 B            | 750 W   | ~400 W  |
| 24 h average (1 h active chat, 23 h bg)       | 620 W   | ~380 W  |
| On battery (Eco mode forces every call to CPU)| UPS trip| 180 W   |

The biggest single win is moving the rotation-driven circuits (`quality_recalc`, `self_synthesis`, `master_loop`, `synapse_prune`, etc.) to CPU with a 3 B/7 B model. The GPU then only fires for user-facing chat and interactive recall.

## Known gaps

- `sidekick` and `sidekick_suggestions` currently fire outside `run_circuit()` scope, so their calls tag as `"unknown"` and stay on GPU. That is the correct behaviour for latency, but means they don't appear under their real name in rollups. To fix, wrap their top-level `async fn` bodies in `CURRENT_CIRCUIT.scope("sidekick".to_string(), ...)`.
- `ThermalThrottle` and `IdleOpportunistic` modes are defined but not auto-triggered. Wiring them needs a GPU-temp vendor SDK and user-presence detection respectively.
- `LoadShed` needs UPS bindings (protocol varies by vendor).
- Wattage coefficients are first-pass estimates. A later calibration pass can swap them for measured wall-power values.
- The dashboard is API-only; there is no HTML UI yet.
