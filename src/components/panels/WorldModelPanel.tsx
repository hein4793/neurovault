import { useState, useEffect } from "react";
import {
  getWorldEntities,
  getCausalLinks,
  getPredictions,
  simulateScenarioCmd,
  WorldEntity,
  CausalLink,
  Prediction,
  ScenarioResult,
} from "@/lib/tauri";

const PREDICTION_STATUS: Record<string, string> = {
  pending: "bg-amber-500/20 text-amber-400",
  validated: "bg-green-500/20 text-green-400",
  invalidated: "bg-red-500/20 text-red-400",
  expired: "bg-brain-border/20 text-brain-muted",
};

const ENTITY_TYPE_COLORS: Record<string, string> = {
  concept: "text-blue-400 bg-blue-500/10 border-blue-500/20",
  technology: "text-cyan-400 bg-cyan-500/10 border-cyan-500/20",
  person: "text-violet-400 bg-violet-500/10 border-violet-500/20",
  organization: "text-amber-400 bg-amber-500/10 border-amber-500/20",
  event: "text-pink-400 bg-pink-500/10 border-pink-500/20",
  process: "text-emerald-400 bg-emerald-500/10 border-emerald-500/20",
};

export function WorldModelPanel() {
  const [entities, setEntities] = useState<WorldEntity[]>([]);
  const [links, setLinks] = useState<CausalLink[]>([]);
  const [predictions, setPredictions] = useState<Prediction[]>([]);
  const [scenario, setScenario] = useState<ScenarioResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [isSimulating, setIsSimulating] = useState(false);
  const [triggerInput, setTriggerInput] = useState("");
  const [activeTab, setActiveTab] = useState<"entities" | "causal" | "predictions" | "simulate">("entities");

  useEffect(() => {
    loadAll();
  }, []);

  const loadAll = async () => {
    setIsLoading(true);
    try {
      const e = await getWorldEntities().catch(() => []);
      setEntities(e);
      const l = await getCausalLinks().catch(() => []);
      setLinks(l);
      const p = await getPredictions().catch(() => []);
      setPredictions(p);
    } catch (err) {
      setError(String(err));
    } finally {
      setIsLoading(false);
    }
  };

  const handleSimulate = async () => {
    if (!triggerInput.trim()) return;
    setIsSimulating(true);
    setError(null);
    try {
      const result = await simulateScenarioCmd(triggerInput.trim());
      setScenario(result);
    } catch (err) {
      setError(String(err));
    } finally {
      setIsSimulating(false);
    }
  };

  const tabs = [
    { id: "entities" as const, label: `Entities (${entities.length})` },
    { id: "causal" as const, label: `Causal (${links.length})` },
    { id: "predictions" as const, label: `Predict (${predictions.length})` },
    { id: "simulate" as const, label: "Simulate" },
  ];

  return (
    <div className="p-4 flex flex-col h-full">
      <h2 className="text-lg font-semibold mb-1 flex items-center gap-2 text-brain-text">
        <svg className="w-5 h-5 text-teal-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3.055 11H5a2 2 0 012 2v1a2 2 0 002 2 2 2 0 012 2v2.945M8 3.935V5.5A2.5 2.5 0 0010.5 8h.5a2 2 0 012 2 2 2 0 104 0 2 2 0 012-2h1.064M15 20.488V18a2 2 0 012-2h3.064M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
        </svg>
        World Model
      </h2>
      <p className="text-[10px] text-brain-muted mb-3">Causal model, predictions, scenario simulation</p>

      {error && (
        <div className="mb-3 text-xs font-mono px-3 py-2 rounded-lg bg-red-500/10 text-red-400 border border-red-500/20">
          {error}
          <button onClick={() => setError(null)} className="ml-2 underline">dismiss</button>
        </div>
      )}

      {/* Tabs */}
      <div className="flex gap-1 mb-3">
        {tabs.map((tab) => (
          <button
            key={tab.id}
            onClick={() => setActiveTab(tab.id)}
            className={`flex-1 text-xs font-mono py-1.5 rounded-lg transition-colors ${
              activeTab === tab.id
                ? "bg-teal-500/20 text-teal-400 border border-teal-500/30"
                : "text-brain-muted hover:text-brain-text border border-brain-border/30"
            }`}
          >
            {tab.label}
          </button>
        ))}
      </div>

      <div className="flex-1 overflow-y-auto space-y-2 min-h-0">
        {isLoading && (
          <div className="text-center text-brain-muted text-sm font-mono py-4 animate-pulse">
            Loading world model...
          </div>
        )}

        {/* Entities Tab */}
        {activeTab === "entities" && (
          <>
            {entities.map((entity) => (
              <div
                key={entity.id}
                className="bg-brain-bg/50 border border-brain-border/30 rounded-lg px-3 py-2"
              >
                <div className="flex items-center justify-between mb-1">
                  <span className="text-xs font-mono font-semibold text-brain-text">{entity.name}</span>
                  <span className={`text-[10px] px-1.5 py-0.5 rounded font-mono border ${
                    ENTITY_TYPE_COLORS[entity.entity_type] || "text-brain-muted bg-brain-border/10 border-brain-border/20"
                  }`}>
                    {entity.entity_type}
                  </span>
                </div>
                {Object.keys(entity.properties).length > 0 && (
                  <div className="space-y-0.5 mt-1">
                    {Object.entries(entity.properties).slice(0, 4).map(([key, val]) => (
                      <div key={key} className="text-[10px] font-mono text-brain-muted/60 flex gap-2">
                        <span className="text-brain-muted/40">{key}:</span>
                        <span className="truncate">{val}</span>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            ))}
            {entities.length === 0 && !isLoading && (
              <div className="text-center text-brain-muted text-xs py-4">No world entities yet</div>
            )}
          </>
        )}

        {/* Causal Links Tab */}
        {activeTab === "causal" && (
          <>
            {links.map((link) => (
              <div
                key={link.id}
                className="bg-brain-bg/50 border border-brain-border/30 rounded-lg px-3 py-2"
              >
                <div className="flex items-center gap-2 text-xs font-mono">
                  <span className="text-teal-400 truncate flex-1">{link.cause}</span>
                  <svg className="w-4 h-4 text-brain-muted/40 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 7l5 5m0 0l-5 5m5-5H6" />
                  </svg>
                  <span className="text-brain-text truncate flex-1">{link.effect}</span>
                </div>
                <div className="flex items-center gap-3 mt-1 text-[10px] font-mono text-brain-muted/60">
                  <span>Strength: </span>
                  <div className="flex-1 h-1.5 bg-brain-border/20 rounded-full overflow-hidden max-w-[100px]">
                    <div
                      className="h-full rounded-full bg-teal-500 transition-all"
                      style={{ width: `${Math.min(100, link.strength * 100)}%` }}
                    />
                  </div>
                  <span>{(link.strength * 100).toFixed(0)}%</span>
                  <span className="text-brain-muted/40">|</span>
                  <span>{link.evidence_count} evidence</span>
                </div>
              </div>
            ))}
            {links.length === 0 && !isLoading && (
              <div className="text-center text-brain-muted text-xs py-4">No causal links discovered yet</div>
            )}
          </>
        )}

        {/* Predictions Tab */}
        {activeTab === "predictions" && (
          <>
            {predictions.map((pred) => (
              <div
                key={pred.id}
                className="bg-brain-bg/50 border border-brain-border/30 rounded-lg px-3 py-2"
              >
                <div className="flex items-start justify-between gap-2 mb-1">
                  <span className="text-xs text-brain-text flex-1">{pred.prediction}</span>
                  <span className={`text-[10px] px-1.5 py-0.5 rounded font-mono flex-shrink-0 ${
                    PREDICTION_STATUS[pred.status] || PREDICTION_STATUS.pending
                  }`}>
                    {pred.status}
                  </span>
                </div>
                <div className="flex items-center gap-3 text-[10px] font-mono text-brain-muted/60">
                  <span className={`${
                    pred.confidence > 0.7 ? "text-green-400" :
                    pred.confidence > 0.4 ? "text-amber-400" : "text-red-400"
                  }`}>
                    {(pred.confidence * 100).toFixed(0)}% conf
                  </span>
                  <span>{pred.timeframe}</span>
                  {pred.validated_at && (
                    <span className="text-brain-muted/40">
                      Validated {new Date(pred.validated_at).toLocaleDateString()}
                    </span>
                  )}
                </div>
              </div>
            ))}
            {predictions.length === 0 && !isLoading && (
              <div className="text-center text-brain-muted text-xs py-4">No predictions yet</div>
            )}
          </>
        )}

        {/* Simulate Tab */}
        {activeTab === "simulate" && (
          <>
            <div className="flex gap-2">
              <input
                type="text"
                value={triggerInput}
                onChange={(e) => setTriggerInput(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && handleSimulate()}
                placeholder="Enter a trigger event..."
                className="flex-1 bg-brain-bg/50 border border-brain-border/30 rounded-lg px-3 py-2 text-xs text-brain-text placeholder-brain-muted/50 focus:outline-none focus:border-teal-500/50"
              />
              <button
                onClick={handleSimulate}
                disabled={isSimulating || !triggerInput.trim()}
                className="px-3 py-2 rounded-lg bg-brain-accent/20 hover:bg-brain-accent/30 text-brain-accent border border-brain-accent/30 text-xs font-mono disabled:opacity-50 transition-colors"
              >
                {isSimulating ? "..." : "Simulate"}
              </button>
            </div>

            {isSimulating && (
              <div className="text-center text-brain-muted text-xs font-mono py-4 animate-pulse">
                Simulating scenario...
              </div>
            )}

            {scenario && !isSimulating && (
              <div className="space-y-2">
                <div className="bg-teal-500/10 border border-teal-500/30 rounded-lg px-3 py-2">
                  <div className="text-[10px] text-teal-400 uppercase tracking-wider mb-0.5 font-semibold">Trigger</div>
                  <div className="text-xs text-brain-text">{scenario.trigger}</div>
                </div>

                <div className="text-[10px] text-brain-muted uppercase tracking-wider font-semibold">
                  Predicted Effects ({scenario.predicted_effects.length})
                </div>

                {scenario.predicted_effects.map((effect, i) => (
                  <div
                    key={i}
                    className="bg-brain-bg/50 border border-brain-border/30 rounded-lg px-3 py-2"
                  >
                    <div className="text-xs text-brain-text mb-1">{effect.effect}</div>
                    <div className="flex items-center gap-3 text-[10px] font-mono text-brain-muted/60">
                      <span className={`${
                        effect.probability > 0.7 ? "text-green-400" :
                        effect.probability > 0.4 ? "text-amber-400" : "text-red-400"
                      }`}>
                        {(effect.probability * 100).toFixed(0)}% likely
                      </span>
                      <span>{effect.timeframe}</span>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </>
        )}
      </div>

      <button
        onClick={loadAll}
        disabled={isLoading}
        className="mt-3 w-full py-2 rounded-lg bg-teal-500/10 text-teal-400 text-xs font-mono hover:bg-teal-500/20 transition-colors border border-teal-500/20 disabled:opacity-50"
      >
        {isLoading ? "Loading..." : "Refresh World Model"}
      </button>
    </div>
  );
}
