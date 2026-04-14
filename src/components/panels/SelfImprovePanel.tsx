import { useState, useEffect } from "react";
import {
  getKnowledgeRules,
  getCircuitPerformance,
  getCapabilities,
  compileRulesNow,
  KnowledgeRule,
  CircuitPerformance,
  Capability,
} from "@/lib/tauri";

const RULE_TYPE_COLORS: Record<string, string> = {
  inference: "bg-blue-500/20 text-blue-400",
  optimization: "bg-emerald-500/20 text-emerald-400",
  heuristic: "bg-amber-500/20 text-amber-400",
  pattern: "bg-purple-500/20 text-purple-400",
  correction: "bg-red-500/20 text-red-400",
  association: "bg-cyan-500/20 text-cyan-400",
};

const CAPABILITY_STATUS: Record<string, string> = {
  mastered: "bg-green-500/20 text-green-400",
  learning: "bg-amber-500/20 text-amber-400",
  gap: "bg-red-500/20 text-red-400",
  emerging: "bg-blue-500/20 text-blue-400",
  stable: "bg-brain-border/20 text-brain-muted",
};

export function SelfImprovePanel() {
  const [rules, setRules] = useState<KnowledgeRule[]>([]);
  const [circuits, setCircuits] = useState<CircuitPerformance[]>([]);
  const [capabilities, setCapabilities] = useState<Capability[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [isCompiling, setIsCompiling] = useState(false);
  const [compileResult, setCompileResult] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState<"rules" | "circuits" | "capabilities">("rules");

  useEffect(() => {
    loadAll();
  }, []);

  const loadAll = async () => {
    setIsLoading(true);
    try {
      const r = await getKnowledgeRules().catch(() => []);
      setRules(r);
      const c = await getCircuitPerformance().catch(() => []);
      setCircuits(c);
      const cap = await getCapabilities().catch(() => []);
      setCapabilities(cap);
    } catch (err) {
      setError(String(err));
    } finally {
      setIsLoading(false);
    }
  };

  const handleCompile = async () => {
    setIsCompiling(true);
    setError(null);
    setCompileResult(null);
    try {
      const result = await compileRulesNow();
      setCompileResult(result);
      loadAll();
    } catch (err) {
      setError(String(err));
    } finally {
      setIsCompiling(false);
    }
  };

  const tabs = [
    { id: "rules" as const, label: `Rules (${rules.length})` },
    { id: "circuits" as const, label: `Circuits (${circuits.length})` },
    { id: "capabilities" as const, label: `Caps (${capabilities.length})` },
  ];

  return (
    <div className="p-4 flex flex-col h-full">
      <h2 className="text-lg font-semibold mb-1 flex items-center gap-2 text-brain-text">
        <svg className="w-5 h-5 text-pink-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
        </svg>
        Self-Improvement
      </h2>
      <p className="text-[10px] text-brain-muted mb-3">Knowledge rules, circuit performance, capability frontier</p>

      {error && (
        <div className="mb-3 text-xs font-mono px-3 py-2 rounded-lg bg-red-500/10 text-red-400 border border-red-500/20">
          {error}
          <button onClick={() => setError(null)} className="ml-2 underline">dismiss</button>
        </div>
      )}

      {compileResult && (
        <div className="mb-3 text-xs font-mono px-3 py-2 rounded-lg bg-green-500/10 text-green-400 border border-green-500/20">
          {compileResult}
          <button onClick={() => setCompileResult(null)} className="ml-2 underline">dismiss</button>
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
                ? "bg-pink-500/20 text-pink-400 border border-pink-500/30"
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
            Loading self-improvement data...
          </div>
        )}

        {/* Rules Tab */}
        {activeTab === "rules" && (
          <>
            {rules.map((rule) => (
              <div
                key={rule.id}
                className="bg-brain-bg/50 border border-brain-border/30 rounded-lg px-3 py-2"
              >
                <div className="flex items-center gap-2 mb-1">
                  <span className={`text-[10px] px-1.5 py-0.5 rounded font-mono ${
                    RULE_TYPE_COLORS[rule.rule_type] || "bg-brain-border/20 text-brain-muted"
                  }`}>
                    {rule.rule_type}
                  </span>
                  <span className="text-[10px] font-mono text-brain-muted/40">{rule.id.slice(0, 8)}</span>
                </div>
                <div className="text-[11px] font-mono text-brain-text mb-0.5">
                  <span className="text-pink-400/70">IF</span> {rule.condition}
                </div>
                <div className="text-[11px] font-mono text-brain-text mb-1">
                  <span className="text-emerald-400/70">THEN</span> {rule.action}
                </div>
                <div className="flex items-center gap-3 text-[10px] font-mono text-brain-muted/60">
                  <span>Confidence: <span className={rule.confidence > 0.7 ? "text-green-400" : rule.confidence > 0.4 ? "text-amber-400" : "text-red-400"}>{(rule.confidence * 100).toFixed(0)}%</span></span>
                  <span>Accuracy: <span className={rule.accuracy > 0.7 ? "text-green-400" : rule.accuracy > 0.4 ? "text-amber-400" : "text-red-400"}>{(rule.accuracy * 100).toFixed(0)}%</span></span>
                  <span>Used {rule.times_applied}x</span>
                </div>
              </div>
            ))}
            {rules.length === 0 && !isLoading && (
              <div className="text-center text-brain-muted text-xs py-4">No rules compiled yet. Click "Compile Rules" below.</div>
            )}
          </>
        )}

        {/* Circuits Tab */}
        {activeTab === "circuits" && (
          <>
            <div className="divide-y divide-brain-border/30">
              {/* Header */}
              <div className="flex items-center gap-2 text-[10px] font-mono text-brain-muted/60 uppercase tracking-wider pb-1.5">
                <span className="flex-1">Circuit</span>
                <span className="w-10 text-right">Runs</span>
                <span className="w-14 text-right">Success</span>
                <span className="w-14 text-right">Avg ms</span>
                <span className="w-14 text-right">Eff.</span>
              </div>
              {circuits.map((circuit) => (
                <div
                  key={circuit.circuit_name}
                  className="flex items-center gap-2 text-[11px] font-mono py-1.5"
                >
                  <span className="flex-1 text-brain-text truncate">{circuit.circuit_name}</span>
                  <span className="w-10 text-right text-brain-muted">{circuit.total_runs}</span>
                  <span className={`w-14 text-right ${
                    circuit.success_rate > 0.9 ? "text-green-400" :
                    circuit.success_rate > 0.7 ? "text-amber-400" : "text-red-400"
                  }`}>
                    {(circuit.success_rate * 100).toFixed(0)}%
                  </span>
                  <span className="w-14 text-right text-brain-muted">
                    {circuit.avg_duration_ms.toFixed(0)}
                  </span>
                  <span className="w-14 text-right">
                    <span className={`${
                      circuit.efficiency_score > 0.8 ? "text-green-400" :
                      circuit.efficiency_score > 0.5 ? "text-amber-400" : "text-red-400"
                    }`}>
                      {(circuit.efficiency_score * 100).toFixed(0)}%
                    </span>
                  </span>
                </div>
              ))}
            </div>
            {circuits.length === 0 && !isLoading && (
              <div className="text-center text-brain-muted text-xs py-4">No circuit performance data yet</div>
            )}
          </>
        )}

        {/* Capabilities Tab */}
        {activeTab === "capabilities" && (
          <>
            {capabilities.map((cap) => (
              <div
                key={cap.name}
                className="bg-brain-bg/50 border border-brain-border/30 rounded-lg px-3 py-2"
              >
                <div className="flex items-center justify-between mb-1.5">
                  <span className="text-xs font-mono font-semibold text-brain-text">{cap.name}</span>
                  <span className={`text-[10px] px-1.5 py-0.5 rounded font-mono ${
                    CAPABILITY_STATUS[cap.status] || CAPABILITY_STATUS.stable
                  }`}>
                    {cap.status}
                  </span>
                </div>
                <div className="flex items-center gap-2">
                  <div className="flex-1 h-2 bg-brain-border/20 rounded-full overflow-hidden">
                    <div
                      className={`h-full rounded-full transition-all duration-500 ${
                        cap.proficiency > 0.8 ? "bg-green-500" :
                        cap.proficiency > 0.5 ? "bg-amber-500" :
                        cap.proficiency > 0.3 ? "bg-orange-500" : "bg-red-500"
                      }`}
                      style={{ width: `${Math.min(100, cap.proficiency * 100)}%` }}
                    />
                  </div>
                  <span className="text-[10px] font-mono text-brain-muted w-10 text-right">
                    {(cap.proficiency * 100).toFixed(0)}%
                  </span>
                </div>
                {cap.last_improved && (
                  <div className="text-[10px] font-mono text-brain-muted/40 mt-1">
                    Last improved: {new Date(cap.last_improved).toLocaleDateString()}
                  </div>
                )}
              </div>
            ))}
            {capabilities.length === 0 && !isLoading && (
              <div className="text-center text-brain-muted text-xs py-4">No capabilities tracked yet</div>
            )}
          </>
        )}
      </div>

      <div className="mt-3 flex gap-2">
        <button
          onClick={handleCompile}
          disabled={isCompiling}
          className="flex-1 py-2 rounded-lg bg-pink-500/10 text-pink-400 text-xs font-mono hover:bg-pink-500/20 transition-colors border border-pink-500/20 disabled:opacity-50"
        >
          {isCompiling ? "Compiling..." : "Compile Rules"}
        </button>
        <button
          onClick={loadAll}
          disabled={isLoading}
          className="flex-1 py-2 rounded-lg bg-brain-accent/10 text-brain-accent text-xs font-mono hover:bg-brain-accent/20 transition-colors border border-brain-accent/20 disabled:opacity-50"
        >
          {isLoading ? "Loading..." : "Refresh"}
        </button>
      </div>
    </div>
  );
}
