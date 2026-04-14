import { useState, useEffect } from "react";
import {
  getSelfModel,
  getAttentionWindow,
  getCuriosityTargetsV2,
  getLearningVelocity,
  SelfModel,
  AttentionNode,
  CuriosityTargetV2,
  LearningVelocity,
} from "@/lib/tauri";

const TREND_ICONS: Record<string, { symbol: string; color: string }> = {
  up: { symbol: "+", color: "text-green-400" },
  down: { symbol: "-", color: "text-red-400" },
  stable: { symbol: "=", color: "text-brain-muted" },
};

const STRATEGY_COLORS: Record<string, string> = {
  strategic: "bg-purple-500/20 text-purple-400 border-purple-500/20",
  serendipitous: "bg-amber-500/20 text-amber-400 border-amber-500/20",
  exploratory: "bg-blue-500/20 text-blue-400 border-blue-500/20",
  deepening: "bg-emerald-500/20 text-emerald-400 border-emerald-500/20",
};

export function ConsciousnessPanel() {
  const [selfModel, setSelfModel] = useState<SelfModel | null>(null);
  const [attention, setAttention] = useState<AttentionNode[]>([]);
  const [curiosity, setCuriosity] = useState<CuriosityTargetV2[]>([]);
  const [velocity, setVelocity] = useState<LearningVelocity[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [activeTab, setActiveTab] = useState<"self" | "attention" | "curiosity" | "velocity">("self");

  useEffect(() => {
    loadAll();
  }, []);

  const loadAll = async () => {
    setIsLoading(true);
    try {
      const sm = await getSelfModel().catch(() => null);
      setSelfModel(sm);
      const att = await getAttentionWindow().catch(() => []);
      setAttention(att);
      const cur = await getCuriosityTargetsV2(20).catch(() => []);
      setCuriosity(cur);
      const vel = await getLearningVelocity().catch(() => []);
      setVelocity(vel);
    } catch (err) {
      setError(String(err));
    } finally {
      setIsLoading(false);
    }
  };

  const tabs = [
    { id: "self" as const, label: "Self-Model" },
    { id: "attention" as const, label: `Focus (${attention.length})` },
    { id: "curiosity" as const, label: `Curiosity (${curiosity.length})` },
    { id: "velocity" as const, label: "Velocity" },
  ];

  return (
    <div className="p-4 flex flex-col h-full">
      <h2 className="text-lg font-semibold mb-1 flex items-center gap-2 text-brain-text">
        <svg className="w-5 h-5 text-purple-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9.663 17h4.673M12 3v1m6.364 1.636l-.707.707M21 12h-1M4 12H3m3.343-5.657l-.707-.707m2.828 9.9a5 5 0 117.072 0l-.548.547A3.374 3.374 0 0014 18.469V19a2 2 0 11-4 0v-.531c0-.895-.356-1.754-.988-2.386l-.548-.547z" />
        </svg>
        Consciousness
      </h2>
      <p className="text-[10px] text-brain-muted mb-3">Self-model, attention, curiosity, learning velocity</p>

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
                ? "bg-purple-500/20 text-purple-400 border border-purple-500/30"
                : "text-brain-muted hover:text-brain-text border border-brain-border/30"
            }`}
          >
            {tab.label}
          </button>
        ))}
      </div>

      <div className="flex-1 overflow-y-auto space-y-3 min-h-0">
        {isLoading && (
          <div className="text-center text-brain-muted text-sm font-mono py-4 animate-pulse">
            Loading consciousness...
          </div>
        )}

        {/* Self-Model Tab */}
        {activeTab === "self" && selfModel && (
          <>
            {/* Identity Card */}
            <div className="bg-gradient-to-br from-purple-500/10 to-violet-500/10 border border-purple-500/30 rounded-lg p-3">
              <div className="text-xs text-brain-text mb-2">{selfModel.identity}</div>
              <div className="text-center my-3">
                <div className="text-3xl font-mono font-bold text-purple-400">{Math.round(selfModel.iq)}</div>
                <div className="text-[10px] text-brain-muted uppercase tracking-wider">Brain IQ</div>
              </div>
            </div>

            {/* Strongest Domains */}
            <div className="bg-brain-bg/50 border border-brain-border/30 rounded-lg p-3">
              <div className="text-[10px] text-brain-muted uppercase tracking-wider mb-1.5 font-semibold">Strongest Domains</div>
              <div className="flex flex-wrap gap-1">
                {selfModel.strongest_domains.map((d) => (
                  <span key={d} className="text-[10px] px-2 py-0.5 rounded bg-green-500/10 text-green-400 border border-green-500/20 font-mono">
                    {d}
                  </span>
                ))}
              </div>
            </div>

            {/* Weakest Domains */}
            <div className="bg-brain-bg/50 border border-brain-border/30 rounded-lg p-3">
              <div className="text-[10px] text-brain-muted uppercase tracking-wider mb-1.5 font-semibold">Weakest Domains</div>
              <div className="flex flex-wrap gap-1">
                {selfModel.weakest_domains.map((d) => (
                  <span key={d} className="text-[10px] px-2 py-0.5 rounded bg-red-500/10 text-red-400 border border-red-500/20 font-mono">
                    {d}
                  </span>
                ))}
              </div>
            </div>

            {/* Bottleneck */}
            <div className="bg-brain-bg/50 border border-brain-border/30 rounded-lg p-3">
              <div className="text-[10px] text-brain-muted uppercase tracking-wider mb-1 font-semibold">Current Bottleneck</div>
              <div className="text-xs text-amber-400 font-mono">{selfModel.bottleneck}</div>
            </div>

            {/* Priorities */}
            <div className="bg-brain-bg/50 border border-brain-border/30 rounded-lg p-3">
              <div className="text-[10px] text-brain-muted uppercase tracking-wider mb-1.5 font-semibold">Priorities</div>
              {selfModel.priorities.map((p, i) => (
                <div key={i} className="text-[11px] text-brain-text flex items-start gap-2 mt-0.5 font-mono">
                  <span className="text-purple-400/60">{i + 1}.</span>
                  <span>{p}</span>
                </div>
              ))}
            </div>

            <div className="text-[10px] text-brain-muted/40 text-center font-mono">
              Last updated: {new Date(selfModel.last_updated).toLocaleString()}
            </div>
          </>
        )}
        {activeTab === "self" && !selfModel && !isLoading && (
          <div className="text-center text-brain-muted text-xs py-4">No self-model generated yet</div>
        )}

        {/* Attention Window Tab */}
        {activeTab === "attention" && (
          <>
            {attention.map((node, i) => {
              const maxScore = attention[0]?.score || 1;
              const barPct = (node.score / maxScore) * 100;
              return (
                <div
                  key={node.node_id}
                  className="bg-brain-bg/50 border border-brain-border/30 rounded-lg px-3 py-2"
                >
                  <div className="flex items-center justify-between mb-1">
                    <span className="text-xs font-mono text-brain-text truncate flex-1">
                      <span className="text-purple-400/50 mr-1">#{i + 1}</span>
                      {node.title}
                    </span>
                    <span className="text-[10px] font-mono text-purple-400 ml-2">
                      {node.score.toFixed(2)}
                    </span>
                  </div>
                  <div className="h-1.5 bg-brain-border/20 rounded-full overflow-hidden mb-1">
                    <div
                      className="h-full rounded-full bg-purple-500 transition-all"
                      style={{ width: `${barPct}%` }}
                    />
                  </div>
                  <div className="text-[10px] font-mono text-brain-muted/50 truncate">
                    {node.reason}
                  </div>
                </div>
              );
            })}
            {attention.length === 0 && !isLoading && (
              <div className="text-center text-brain-muted text-xs py-4">Attention window empty</div>
            )}
          </>
        )}

        {/* Curiosity Targets Tab */}
        {activeTab === "curiosity" && (
          <>
            {curiosity.map((target, i) => (
              <div
                key={i}
                className="bg-brain-bg/50 border border-brain-border/30 rounded-lg px-3 py-2"
              >
                <div className="flex items-center gap-2 mb-1">
                  <span className={`text-[9px] px-1.5 py-0.5 rounded font-mono border ${
                    STRATEGY_COLORS[target.strategy] || "bg-brain-border/20 text-brain-muted border-brain-border/20"
                  }`}>
                    {target.strategy}
                  </span>
                  <span className="text-xs font-mono text-brain-text truncate flex-1">{target.topic}</span>
                </div>
                <div className="flex items-center gap-3 text-[10px] font-mono text-brain-muted/60">
                  <span>Expected gain: <span className={
                    target.expected_gain > 0.7 ? "text-green-400" :
                    target.expected_gain > 0.4 ? "text-amber-400" : "text-brain-muted"
                  }>{(target.expected_gain * 100).toFixed(0)}%</span></span>
                </div>
                <div className="text-[10px] font-mono text-brain-muted/40 mt-0.5 truncate">
                  {target.reason}
                </div>
              </div>
            ))}
            {curiosity.length === 0 && !isLoading && (
              <div className="text-center text-brain-muted text-xs py-4">No curiosity targets yet</div>
            )}
          </>
        )}

        {/* Learning Velocity Tab */}
        {activeTab === "velocity" && (
          <>
            {velocity.map((v) => {
              const trend = TREND_ICONS[v.trend] || TREND_ICONS.stable;
              return (
                <div
                  key={v.domain}
                  className="bg-brain-bg/50 border border-brain-border/30 rounded-lg px-3 py-2"
                >
                  <div className="flex items-center justify-between mb-1.5">
                    <span className="text-xs font-mono font-semibold text-brain-text">{v.domain}</span>
                    <span className={`text-[10px] font-mono ${trend.color}`}>
                      {trend.symbol} {v.trend}
                    </span>
                  </div>
                  <div className="flex items-center gap-2">
                    <div className="flex-1 h-2 bg-brain-border/20 rounded-full overflow-hidden">
                      <div
                        className={`h-full rounded-full transition-all duration-500 ${
                          v.velocity > 0.7 ? "bg-green-500" :
                          v.velocity > 0.4 ? "bg-amber-500" :
                          v.velocity > 0.2 ? "bg-orange-500" : "bg-red-500"
                        }`}
                        style={{ width: `${Math.min(100, v.velocity * 100)}%` }}
                      />
                    </div>
                    <span className="text-[10px] font-mono text-brain-muted w-10 text-right">
                      {(v.velocity * 100).toFixed(0)}%
                    </span>
                  </div>
                </div>
              );
            })}
            {velocity.length === 0 && !isLoading && (
              <div className="text-center text-brain-muted text-xs py-4">No velocity data yet</div>
            )}
          </>
        )}
      </div>

      <button
        onClick={loadAll}
        disabled={isLoading}
        className="mt-3 w-full py-2 rounded-lg bg-purple-500/10 text-purple-400 text-xs font-mono hover:bg-purple-500/20 transition-colors border border-purple-500/20 disabled:opacity-50"
      >
        {isLoading ? "Loading..." : "Refresh Consciousness"}
      </button>
    </div>
  );
}
