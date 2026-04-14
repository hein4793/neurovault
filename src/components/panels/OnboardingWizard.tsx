import { useState, useEffect, useCallback } from "react";
import {
  checkOllamaStatus,
  pullOllamaModel,
  completeSetup,
  detectAiAssistantDirs,
  OllamaModelInfo,
} from "@/lib/tauri";
import { open } from "@tauri-apps/plugin-dialog";

interface OnboardingWizardProps {
  onComplete: () => void;
}

const TOTAL_STEPS = 5;

const RECOMMENDED_MODELS = [
  { name: "nomic-embed-text", purpose: "Embedding (required)" },
  { name: "qwen2.5-coder:14b", purpose: "Fast LLM for reasoning" },
];

export function OnboardingWizard({ onComplete }: OnboardingWizardProps) {
  const [step, setStep] = useState(1);

  // Step 1 state
  const [ollamaReachable, setOllamaReachable] = useState<boolean | null>(null);
  const [ollamaChecking, setOllamaChecking] = useState(false);

  // Step 2 state
  const [installedModels, setInstalledModels] = useState<OllamaModelInfo[]>([]);
  const [pullingModel, setPullingModel] = useState<string | null>(null);
  const [pullStatus, setPullStatus] = useState<Record<string, string>>({});

  // Step 3 state
  const [enableAiSync, setEnableAiSync] = useState(true);
  const [enableFileWatcher, setEnableFileWatcher] = useState(false);
  const [watchedPaths, setWatchedPaths] = useState<string[]>([]);
  const [detectedDirs, setDetectedDirs] = useState<string[]>([]);

  // Step 4 state
  const [brainName, setBrainName] = useState("My Brain");

  // Step 5 state
  const [completing, setCompleting] = useState(false);

  const checkOllama = useCallback(async () => {
    setOllamaChecking(true);
    try {
      const status = await checkOllamaStatus();
      setOllamaReachable(status.reachable);
      setInstalledModels(status.models);
    } catch {
      setOllamaReachable(false);
    } finally {
      setOllamaChecking(false);
    }
  }, []);

  // Check Ollama on mount
  useEffect(() => {
    checkOllama();
    detectAiAssistantDirs()
      .then(setDetectedDirs)
      .catch(() => {});
  }, [checkOllama]);

  const handlePullModel = async (modelName: string) => {
    setPullingModel(modelName);
    setPullStatus((prev) => ({ ...prev, [modelName]: "pulling..." }));
    try {
      const result = await pullOllamaModel(modelName);
      if (result.completed) {
        setPullStatus((prev) => ({ ...prev, [modelName]: "installed" }));
        // Refresh model list
        const status = await checkOllamaStatus();
        setInstalledModels(status.models);
      } else {
        setPullStatus((prev) => ({
          ...prev,
          [modelName]: result.error || "failed",
        }));
      }
    } catch (err) {
      setPullStatus((prev) => ({ ...prev, [modelName]: `error: ${err}` }));
    } finally {
      setPullingModel(null);
    }
  };

  const handleAddFolder = async () => {
    try {
      const selected = await open({ directory: true, multiple: false });
      if (selected && typeof selected === "string") {
        setWatchedPaths((prev) =>
          prev.includes(selected) ? prev : [...prev, selected]
        );
      }
    } catch {
      // User cancelled
    }
  };

  const handleRemovePath = (path: string) => {
    setWatchedPaths((prev) => prev.filter((p) => p !== path));
  };

  const handleComplete = async () => {
    setCompleting(true);
    try {
      await completeSetup({
        brainName,
        enableAiAssistantSync: enableAiSync,
        enableFileWatcher,
        watchedPaths,
      });
      onComplete();
    } catch (err) {
      console.error("Failed to complete setup:", err);
      setCompleting(false);
    }
  };

  const isModelInstalled = (name: string) =>
    installedModels.some((m) => m.name === name || m.name.startsWith(name.split(":")[0]));

  const canProceedFromStep1 = ollamaReachable === true;

  return (
    <div className="fixed inset-0 z-[9999] flex items-center justify-center bg-brain-bg">
      {/* Subtle background glow */}
      <div className="absolute inset-0 overflow-hidden pointer-events-none">
        <div className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-[600px] h-[600px] rounded-full bg-brain-accent/5 blur-[120px]" />
      </div>

      <div className="relative w-full max-w-[560px] mx-4">
        {/* Progress bar */}
        <div className="flex items-center gap-2 mb-6 px-2">
          {Array.from({ length: TOTAL_STEPS }, (_, i) => (
            <div key={i} className="flex-1 flex items-center gap-2">
              <div
                className={`h-1 flex-1 rounded-full transition-colors duration-300 ${
                  i + 1 <= step
                    ? "bg-brain-accent"
                    : "bg-brain-border/50"
                }`}
              />
            </div>
          ))}
          <span className="text-[10px] text-brain-muted font-mono ml-1">
            {step}/{TOTAL_STEPS}
          </span>
        </div>

        {/* Card */}
        <div className="glass-panel p-8">
          {/* ===== STEP 1: Welcome ===== */}
          {step === 1 && (
            <div className="flex flex-col">
              <div className="flex items-center gap-3 mb-2">
                <div className="w-10 h-10 rounded-xl bg-brain-accent/10 border border-brain-accent/20 flex items-center justify-center">
                  <svg className="w-5 h-5 text-brain-accent" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M9.663 17h4.673M12 3v1m6.364 1.636l-.707.707M21 12h-1M4 12H3m3.343-5.657l-.707-.707m2.828 9.9a5 5 0 117.072 0l-.548.547A3.374 3.374 0 0014 18.469V19a2 2 0 11-4 0v-.531c0-.895-.356-1.754-.988-2.386l-.548-.547z" />
                  </svg>
                </div>
                <h1 className="text-2xl font-semibold text-brain-text">
                  Welcome to NeuroVault
                </h1>
              </div>

              <p className="text-sm text-brain-muted leading-relaxed mb-6 ml-[52px]">
                A self-evolving 3D knowledge brain that runs entirely on your
                machine. It learns from your work, connects ideas, and grows
                smarter over time.
              </p>

              {/* Ollama status */}
              <div className="rounded-xl border border-brain-border/50 bg-brain-bg/50 p-4 mb-6">
                <div className="flex items-center justify-between mb-2">
                  <span className="text-sm font-mono text-brain-text">Ollama Status</span>
                  <button
                    onClick={checkOllama}
                    disabled={ollamaChecking}
                    className="text-[10px] text-brain-accent font-mono hover:underline disabled:opacity-50"
                  >
                    {ollamaChecking ? "Checking..." : "Re-check"}
                  </button>
                </div>

                {ollamaReachable === null && (
                  <div className="flex items-center gap-2 text-brain-muted text-xs font-mono">
                    <div className="w-3 h-3 border border-brain-muted/50 border-t-brain-muted rounded-full animate-spin" />
                    Detecting Ollama...
                  </div>
                )}
                {ollamaReachable === true && (
                  <div className="flex items-center gap-2 text-emerald-400 text-xs font-mono">
                    <div className="w-2 h-2 rounded-full bg-emerald-400" />
                    Connected ({installedModels.length} model{installedModels.length !== 1 ? "s" : ""} installed)
                  </div>
                )}
                {ollamaReachable === false && (
                  <div className="space-y-2">
                    <div className="flex items-center gap-2 text-amber-400 text-xs font-mono">
                      <div className="w-2 h-2 rounded-full bg-amber-400" />
                      Ollama not detected
                    </div>
                    <p className="text-[11px] text-brain-muted leading-relaxed">
                      NeuroVault needs Ollama for AI-powered features.
                      Install it from{" "}
                      <a
                        href="https://ollama.com"
                        target="_blank"
                        rel="noopener noreferrer"
                        className="text-brain-accent hover:underline"
                      >
                        ollama.com
                      </a>
                      , then click Re-check.
                    </p>
                  </div>
                )}
              </div>

              <div className="flex justify-end">
                <button
                  onClick={() => setStep(2)}
                  disabled={!canProceedFromStep1}
                  className="px-6 py-2.5 rounded-lg bg-brain-accent/20 text-brain-accent text-sm font-mono border border-brain-accent/30 hover:bg-brain-accent/30 transition-colors disabled:opacity-30 disabled:cursor-not-allowed"
                >
                  Continue
                </button>
              </div>
            </div>
          )}

          {/* ===== STEP 2: Models ===== */}
          {step === 2 && (
            <div className="flex flex-col">
              <h2 className="text-xl font-semibold text-brain-text mb-1">AI Models</h2>
              <p className="text-sm text-brain-muted mb-6">
                NeuroVault uses local AI models via Ollama. Pull the recommended models below, or skip if you already have them.
              </p>

              <div className="space-y-3 mb-6">
                {RECOMMENDED_MODELS.map((rec) => {
                  const installed = isModelInstalled(rec.name);
                  const status = pullStatus[rec.name];
                  const isPulling = pullingModel === rec.name;

                  return (
                    <div
                      key={rec.name}
                      className="flex items-center justify-between rounded-xl border border-brain-border/50 bg-brain-bg/50 p-4"
                    >
                      <div>
                        <div className="text-sm font-mono text-brain-text">{rec.name}</div>
                        <div className="text-[11px] text-brain-muted">{rec.purpose}</div>
                      </div>
                      <div className="flex items-center gap-2">
                        {installed || status === "installed" ? (
                          <span className="text-xs text-emerald-400 font-mono flex items-center gap-1">
                            <svg className="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
                            </svg>
                            Installed
                          </span>
                        ) : isPulling ? (
                          <span className="text-xs text-brain-accent font-mono flex items-center gap-1">
                            <div className="w-3 h-3 border border-brain-accent/50 border-t-brain-accent rounded-full animate-spin" />
                            Pulling...
                          </span>
                        ) : status && status !== "installed" ? (
                          <span className="text-xs text-amber-400 font-mono truncate max-w-[120px]">
                            {status}
                          </span>
                        ) : (
                          <button
                            onClick={() => handlePullModel(rec.name)}
                            disabled={pullingModel !== null}
                            className="text-xs px-3 py-1.5 rounded-lg bg-brain-accent/10 text-brain-accent font-mono border border-brain-accent/20 hover:bg-brain-accent/20 transition-colors disabled:opacity-30"
                          >
                            Pull
                          </button>
                        )}
                      </div>
                    </div>
                  );
                })}
              </div>

              {/* Show all installed models */}
              {installedModels.length > 0 && (
                <div className="mb-6">
                  <div className="text-[10px] text-brain-muted font-mono uppercase tracking-wider mb-2">
                    All installed models
                  </div>
                  <div className="flex flex-wrap gap-1.5">
                    {installedModels.map((m) => (
                      <span
                        key={m.name}
                        className="text-[10px] px-2 py-1 rounded-md bg-brain-border/30 text-brain-muted font-mono"
                      >
                        {m.name}
                      </span>
                    ))}
                  </div>
                </div>
              )}

              <div className="flex justify-between">
                <button
                  onClick={() => setStep(1)}
                  className="px-4 py-2.5 rounded-lg text-brain-muted text-sm font-mono hover:text-brain-text transition-colors"
                >
                  Back
                </button>
                <button
                  onClick={() => setStep(3)}
                  className="px-6 py-2.5 rounded-lg bg-brain-accent/20 text-brain-accent text-sm font-mono border border-brain-accent/30 hover:bg-brain-accent/30 transition-colors"
                >
                  Continue
                </button>
              </div>
            </div>
          )}

          {/* ===== STEP 3: Knowledge Sources ===== */}
          {step === 3 && (
            <div className="flex flex-col">
              <h2 className="text-xl font-semibold text-brain-text mb-1">Knowledge Sources</h2>
              <p className="text-sm text-brain-muted mb-6">
                Choose where NeuroVault should look for knowledge to ingest. You can always change these later in Settings.
              </p>

              <div className="space-y-4 mb-6">
                {/* AI assistant sync toggle */}
                <div className="rounded-xl border border-brain-border/50 bg-brain-bg/50 p-4">
                  <div className="flex items-center justify-between">
                    <div className="flex-1 mr-4">
                      <div className="text-sm font-mono text-brain-text">Watch AI assistant chat history</div>
                      <div className="text-[11px] text-brain-muted mt-0.5">
                        Auto-import conversations from AI coding assistants
                      </div>
                      {detectedDirs.length > 0 && (
                        <div className="mt-2 space-y-1">
                          {detectedDirs.map((d) => (
                            <div key={d} className="text-[10px] text-emerald-400/80 font-mono flex items-center gap-1">
                              <div className="w-1.5 h-1.5 rounded-full bg-emerald-400" />
                              {d}
                            </div>
                          ))}
                        </div>
                      )}
                    </div>
                    <Toggle checked={enableAiSync} onChange={setEnableAiSync} />
                  </div>
                </div>

                {/* Custom folder toggle */}
                <div className="rounded-xl border border-brain-border/50 bg-brain-bg/50 p-4">
                  <div className="flex items-center justify-between mb-3">
                    <div className="flex-1 mr-4">
                      <div className="text-sm font-mono text-brain-text">Watch custom folders</div>
                      <div className="text-[11px] text-brain-muted mt-0.5">
                        Monitor directories for documents, notes, or code
                      </div>
                    </div>
                    <Toggle checked={enableFileWatcher} onChange={setEnableFileWatcher} />
                  </div>

                  {enableFileWatcher && (
                    <div className="space-y-2">
                      {watchedPaths.map((p) => (
                        <div
                          key={p}
                          className="flex items-center justify-between text-[11px] font-mono bg-brain-bg/80 border border-brain-border/30 rounded-lg px-3 py-2"
                        >
                          <span className="text-brain-muted truncate flex-1 mr-2">{p}</span>
                          <button
                            onClick={() => handleRemovePath(p)}
                            className="text-brain-muted/50 hover:text-red-400 transition-colors shrink-0"
                          >
                            <svg className="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                            </svg>
                          </button>
                        </div>
                      ))}
                      <button
                        onClick={handleAddFolder}
                        className="w-full text-xs px-3 py-2 rounded-lg border border-dashed border-brain-border/50 text-brain-muted font-mono hover:border-brain-accent/30 hover:text-brain-accent transition-colors"
                      >
                        + Add folder
                      </button>
                    </div>
                  )}
                </div>

                {/* Manual only hint */}
                <div className="text-[11px] text-brain-muted/60 font-mono px-1">
                  You can also add knowledge manually via the Ingest panel at any time.
                </div>
              </div>

              <div className="flex justify-between">
                <button
                  onClick={() => setStep(2)}
                  className="px-4 py-2.5 rounded-lg text-brain-muted text-sm font-mono hover:text-brain-text transition-colors"
                >
                  Back
                </button>
                <button
                  onClick={() => setStep(4)}
                  className="px-6 py-2.5 rounded-lg bg-brain-accent/20 text-brain-accent text-sm font-mono border border-brain-accent/30 hover:bg-brain-accent/30 transition-colors"
                >
                  Continue
                </button>
              </div>
            </div>
          )}

          {/* ===== STEP 4: Name your brain ===== */}
          {step === 4 && (
            <div className="flex flex-col">
              <h2 className="text-xl font-semibold text-brain-text mb-1">Name Your Brain</h2>
              <p className="text-sm text-brain-muted mb-6">
                Give your knowledge brain a name. This is just for you.
              </p>

              <div className="mb-8">
                <input
                  type="text"
                  value={brainName}
                  onChange={(e) => setBrainName(e.target.value)}
                  placeholder="My Brain"
                  maxLength={64}
                  className="w-full text-lg bg-brain-bg/50 border border-brain-border/50 rounded-xl px-4 py-3 text-brain-text font-mono outline-none focus:border-brain-accent/50 transition-colors text-center"
                  autoFocus
                />
                <div className="text-[10px] text-brain-muted/50 font-mono text-center mt-2">
                  {brainName.length}/64 characters
                </div>
              </div>

              <div className="flex justify-between">
                <button
                  onClick={() => setStep(3)}
                  className="px-4 py-2.5 rounded-lg text-brain-muted text-sm font-mono hover:text-brain-text transition-colors"
                >
                  Back
                </button>
                <button
                  onClick={() => setStep(5)}
                  disabled={!brainName.trim()}
                  className="px-6 py-2.5 rounded-lg bg-brain-accent/20 text-brain-accent text-sm font-mono border border-brain-accent/30 hover:bg-brain-accent/30 transition-colors disabled:opacity-30 disabled:cursor-not-allowed"
                >
                  Continue
                </button>
              </div>
            </div>
          )}

          {/* ===== STEP 5: Ready ===== */}
          {step === 5 && (
            <div className="flex flex-col items-center text-center">
              <div className="w-16 h-16 rounded-2xl bg-brain-accent/10 border border-brain-accent/20 flex items-center justify-center mb-4">
                <svg className="w-8 h-8 text-brain-accent" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M13 10V3L4 14h7v7l9-11h-7z" />
                </svg>
              </div>

              <h2 className="text-xl font-semibold text-brain-text mb-2">
                {brainName} is ready
              </h2>
              <p className="text-sm text-brain-muted mb-8 max-w-sm leading-relaxed">
                Your brain will start learning immediately. As you work, it
                will connect ideas, find patterns, and grow smarter over time.
              </p>

              {/* Summary */}
              <div className="w-full rounded-xl border border-brain-border/50 bg-brain-bg/50 p-4 mb-8 text-left">
                <div className="text-[10px] text-brain-muted font-mono uppercase tracking-wider mb-3">
                  Configuration summary
                </div>
                <div className="space-y-2 text-xs font-mono">
                  <SummaryRow label="Brain name" value={brainName} />
                  <SummaryRow
                    label="AI models"
                    value={
                      installedModels.length > 0
                        ? `${installedModels.length} installed`
                        : "None yet"
                    }
                  />
                  <SummaryRow
                    label="AI chat sync"
                    value={enableAiSync ? "Enabled" : "Disabled"}
                  />
                  <SummaryRow
                    label="File watcher"
                    value={
                      enableFileWatcher
                        ? `${watchedPaths.length} folder${watchedPaths.length !== 1 ? "s" : ""}`
                        : "Disabled"
                    }
                  />
                </div>
              </div>

              <div className="flex items-center gap-3 w-full">
                <button
                  onClick={() => setStep(4)}
                  className="px-4 py-2.5 rounded-lg text-brain-muted text-sm font-mono hover:text-brain-text transition-colors"
                >
                  Back
                </button>
                <button
                  onClick={handleComplete}
                  disabled={completing}
                  className="flex-1 py-3 rounded-xl bg-gradient-to-r from-brain-accent/20 to-cyan-500/20 text-brain-accent text-sm font-semibold font-mono border border-brain-accent/30 hover:from-brain-accent/30 hover:to-cyan-500/30 transition-all disabled:opacity-50"
                >
                  {completing ? (
                    <span className="flex items-center justify-center gap-2">
                      <div className="w-3.5 h-3.5 border border-brain-accent/50 border-t-brain-accent rounded-full animate-spin" />
                      Starting...
                    </span>
                  ) : (
                    "Start your brain"
                  )}
                </button>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

// ===== Helper components =====

function Toggle({
  checked,
  onChange,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <div
      onClick={() => onChange(!checked)}
      className={`w-10 h-5 rounded-full transition-colors relative cursor-pointer shrink-0 ${
        checked ? "bg-brain-accent/40" : "bg-brain-border/50"
      }`}
    >
      <div
        className={`absolute top-0.5 w-4 h-4 rounded-full transition-all ${
          checked ? "left-5 bg-brain-accent" : "left-0.5 bg-brain-muted"
        }`}
      />
    </div>
  );
}

function SummaryRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between">
      <span className="text-brain-muted">{label}</span>
      <span className="text-brain-text">{value}</span>
    </div>
  );
}
