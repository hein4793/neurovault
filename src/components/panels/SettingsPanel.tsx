import { useState, useEffect } from "react";
import {
  getSettings,
  updateSettings,
  clearCache,
  getBrainVersion,
  listInstalledModels,
  BrainSettings,
  InstalledModel,
} from "@/lib/tauri";

export function SettingsPanel() {
  const [settings, setSettings] = useState<BrainSettings | null>(null);
  const [version, setVersion] = useState("0.1.0");
  const [status, setStatus] = useState("");
  const [saving, setSaving] = useState(false);
  // Phase 3.1 — installed Ollama models for the dropdowns
  const [installedModels, setInstalledModels] = useState<InstalledModel[]>([]);
  const [ollamaReachable, setOllamaReachable] = useState<boolean | null>(null);

  useEffect(() => {
    getSettings().then(setSettings).catch(console.error);
    getBrainVersion().then(setVersion).catch(() => {});
    // Load installed Ollama models for the multi-model dropdowns.
    // Best-effort — failure is non-fatal, the user can still type.
    listInstalledModels()
      .then((r) => {
        setInstalledModels(r.models);
        setOllamaReachable(r.reachable);
      })
      .catch(() => setOllamaReachable(false));
  }, []);

  const handleSave = async () => {
    if (!settings) return;
    setSaving(true);
    try {
      const updated = await updateSettings(settings);
      setSettings(updated);
      setStatus("Settings saved");
      setTimeout(() => setStatus(""), 2000);
    } catch (err) {
      setStatus(`Error: ${err}`);
    } finally {
      setSaving(false);
    }
  };

  const handleClearCache = async () => {
    try {
      const msg = await clearCache();
      setStatus(msg);
      setTimeout(() => setStatus(""), 2000);
    } catch (err) {
      setStatus(`Error: ${err}`);
    }
  };

  if (!settings) {
    return (
      <div className="p-4 flex items-center justify-center h-full">
        <span className="text-brain-muted text-sm font-mono animate-pulse">Loading settings...</span>
      </div>
    );
  }

  return (
    <div className="p-4 flex flex-col h-full">
      <h2 className="text-lg font-semibold mb-4 flex items-center gap-2 text-brain-text">
        <svg className="w-5 h-5 text-brain-muted" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.066 2.573c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.573 1.066c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.066-2.573c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
        </svg>
        Settings
      </h2>

      {status && (
        <div className="mb-3 text-xs font-mono px-3 py-2 rounded-lg bg-brain-accent/10 text-brain-accent border border-brain-accent/20">
          {status}
        </div>
      )}

      <div className="flex-1 overflow-y-auto space-y-4">
        {/* AI & Embeddings */}
        <section>
          <h3 className="text-xs font-semibold text-brain-muted uppercase tracking-wider mb-2">
            AI & Embeddings
          </h3>
          <div className="space-y-3">
            <Field label="Ollama URL" value={settings.ollama_url} onChange={(v) => setSettings({ ...settings, ollama_url: v })} />
            <Field label="Embedding Model" value={settings.embedding_model} onChange={(v) => setSettings({ ...settings, embedding_model: v })} />
            <div>
              <label className="text-xs text-brain-muted font-mono block mb-1">LLM Provider</label>
              <select
                value={settings.llm_provider}
                onChange={(e) => setSettings({ ...settings, llm_provider: e.target.value })}
                className="w-full text-sm bg-brain-bg/50 border border-brain-border/50 rounded-lg px-3 py-2 text-brain-text font-mono outline-none focus:border-brain-accent/50"
              >
                <option value="ollama">Ollama (Local)</option>
                <option value="anthropic">Anthropic Claude API</option>
              </select>
            </div>
            <Field label="Default LLM Model" value={settings.llm_model} onChange={(v) => setSettings({ ...settings, llm_model: v })} />
          </div>
        </section>

        {/* Phase 2.6 / 3.1 — Multi-model tier routing */}
        <section>
          <div className="flex items-center justify-between mb-2">
            <h3 className="text-xs font-semibold text-brain-muted uppercase tracking-wider">
              Multi-Model Routing
            </h3>
            {ollamaReachable === false && (
              <span className="text-[10px] text-amber-400 font-mono">Ollama unreachable</span>
            )}
            {ollamaReachable === true && (
              <span className="text-[10px] text-emerald-400 font-mono">{installedModels.length} models</span>
            )}
          </div>

          <div className="text-[10px] text-brain-muted/70 font-mono mb-2 leading-relaxed">
            High-frequency circuits use the FAST model. Deep reasoning circuits use the DEEP model.
            Picking the right model per tier balances speed and quality.
          </div>

          <div className="space-y-3">
            <ModelDropdown
              label="Fast Model"
              hint="user_pattern_mining · decision_extractor · code_patterns · meta_reflection · self_assessment · master_loop"
              value={settings.llm_model_fast || settings.llm_model}
              onChange={(v) => setSettings({ ...settings, llm_model_fast: v })}
              models={installedModels}
              ollamaReachable={ollamaReachable}
            />

            <ModelDropdown
              label="Deep Model"
              hint="self_synthesis · knowledge_synthesizer · compression · contradiction · hypothesis · prediction · cross_domain"
              value={settings.llm_model_deep || settings.llm_model}
              onChange={(v) => setSettings({ ...settings, llm_model_deep: v })}
              models={installedModels}
              ollamaReachable={ollamaReachable}
            />
          </div>
        </section>

        {/* Sync */}
        <section>
          <h3 className="text-xs font-semibold text-brain-muted uppercase tracking-wider mb-2">
            Sync & Import
          </h3>
          <label className="flex items-center gap-3 cursor-pointer">
            <div
              onClick={() => setSettings({ ...settings, auto_sync_enabled: !settings.auto_sync_enabled })}
              className={`w-10 h-5 rounded-full transition-colors relative cursor-pointer ${
                settings.auto_sync_enabled ? "bg-brain-accent/40" : "bg-brain-border/50"
              }`}
            >
              <div
                className={`absolute top-0.5 w-4 h-4 rounded-full transition-all ${
                  settings.auto_sync_enabled ? "left-5 bg-brain-accent" : "left-0.5 bg-brain-muted"
                }`}
              />
            </div>
            <span className="text-sm text-brain-text font-mono">Auto-sync file changes</span>
          </label>
        </section>

        {/* Storage */}
        <section>
          <h3 className="text-xs font-semibold text-brain-muted uppercase tracking-wider mb-2">
            Storage
          </h3>
          <div className="text-xs font-mono text-brain-muted bg-brain-bg/50 border border-brain-border/30 rounded-lg px-3 py-2 break-all">
            {settings.data_dir || "~/.neurovault"}
          </div>
          <button
            onClick={handleClearCache}
            className="mt-2 text-xs px-3 py-1.5 rounded-lg bg-brain-panel text-brain-muted hover:text-brain-text transition-colors border border-brain-border/50 font-mono"
          >
            Clear Cache
          </button>
        </section>

        {/* About */}
        <section>
          <h3 className="text-xs font-semibold text-brain-muted uppercase tracking-wider mb-2">
            About
          </h3>
          <div className="space-y-1 text-xs font-mono text-brain-muted">
            <div>NeuroVault v{version}</div>
            <div>Tauri v2 + React 19 + Three.js</div>
            <div>SQLite + Ollama</div>
          </div>
        </section>
      </div>

      {/* Save button */}
      <button
        onClick={handleSave}
        disabled={saving}
        className="mt-4 w-full py-2.5 rounded-lg bg-gradient-to-r from-brain-accent/20 to-brain-research/20 text-brain-text text-sm font-mono hover:from-brain-accent/30 hover:to-brain-research/30 transition-all disabled:opacity-50 border border-brain-accent/20"
      >
        {saving ? "Saving..." : "Save Settings"}
      </button>
    </div>
  );
}

function Field({
  label,
  value,
  onChange,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
}) {
  return (
    <div>
      <label className="text-xs text-brain-muted font-mono block mb-1">{label}</label>
      <input
        type="text"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className="w-full text-sm bg-brain-bg/50 border border-brain-border/50 rounded-lg px-3 py-2 text-brain-text font-mono outline-none focus:border-brain-accent/50"
      />
    </div>
  );
}

/// Phase 3.1 — Smart dropdown for picking an installed Ollama model.
/// Falls back to a free-text input when Ollama is unreachable so the
/// user can still set the field manually.
function ModelDropdown({
  label,
  hint,
  value,
  onChange,
  models,
  ollamaReachable,
}: {
  label: string;
  hint: string;
  value: string;
  onChange: (v: string) => void;
  models: InstalledModel[];
  ollamaReachable: boolean | null;
}) {
  // Filter out embedding models from the chat-model dropdown
  const chatModels = models.filter(
    (m) => !m.family.includes("embed") && !m.family.includes("bge"),
  );
  // Surface qwen2.5-coder first, then alphabetic
  const sorted = [...chatModels].sort((a, b) => {
    const qA = a.family.startsWith("qwen2.5-coder") ? 0 : 1;
    const qB = b.family.startsWith("qwen2.5-coder") ? 0 : 1;
    if (qA !== qB) return qA - qB;
    return a.name.localeCompare(b.name);
  });

  const valueIsInList = sorted.some((m) => m.name === value);

  return (
    <div>
      <label className="text-xs text-brain-muted font-mono block mb-1">{label}</label>
      {ollamaReachable && sorted.length > 0 ? (
        <select
          value={value}
          onChange={(e) => onChange(e.target.value)}
          className="w-full text-sm bg-brain-bg/50 border border-brain-border/50 rounded-lg px-3 py-2 text-brain-text font-mono outline-none focus:border-brain-accent/50"
        >
          {!valueIsInList && value && (
            <option value={value}>{value}  (not installed)</option>
          )}
          {sorted.map((m) => (
            <option key={m.name} value={m.name}>
              {m.name}  ·  {humanSize(m.size)}
            </option>
          ))}
        </select>
      ) : (
        <input
          type="text"
          value={value}
          onChange={(e) => onChange(e.target.value)}
          placeholder="qwen2.5-coder:14b"
          className="w-full text-sm bg-brain-bg/50 border border-brain-border/50 rounded-lg px-3 py-2 text-brain-text font-mono outline-none focus:border-brain-accent/50"
        />
      )}
      <div className="text-[9px] text-brain-muted/50 font-mono mt-1 leading-relaxed">{hint}</div>
    </div>
  );
}

function humanSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(0)} MB`;
  return `${(bytes / 1024 / 1024 / 1024).toFixed(1)} GB`;
}
