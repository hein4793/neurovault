import { useState, useEffect } from "react";
import {
  analyzePatterns,
  analyzeTrends,
  getRecommendations,
  boostIq,
  maximizeIq,
  PatternReport,
  TrendReport,
  Recommendation,
  BrainIqBreakdown,
  getDomainColor,
} from "@/lib/tauri";

export function InsightsPanel() {
  const [patterns, setPatterns] = useState<PatternReport | null>(null);
  const [trends, setTrends] = useState<TrendReport | null>(null);
  const [recs, setRecs] = useState<Recommendation[]>([]);
  const [activeTab, setActiveTab] = useState<"overview" | "patterns" | "recs">("overview");
  const [boostStatus, setBoostStatus] = useState("");
  const [isLoading, setIsLoading] = useState(false);

  useEffect(() => {
    loadAll();
  }, []);

  const loadAll = async () => {
    setIsLoading(true);
    try {
      // Sequential — SurrealDB locks prevent parallel queries on 199K nodes
      const t = await analyzeTrends().catch(() => null);
      setTrends(t);
      const r = await getRecommendations().catch(() => []);
      setRecs(r);
      const p = await analyzePatterns().catch(() => null);
      setPatterns(p);
    } finally {
      setIsLoading(false);
    }
  };

  const tabs = [
    { id: "overview" as const, label: "Overview" },
    { id: "patterns" as const, label: "Patterns" },
    { id: "recs" as const, label: `Recs (${recs.length})` },
  ];

  return (
    <div className="p-4 flex flex-col h-full">
      <h2 className="text-lg font-semibold mb-4 flex items-center gap-2 text-brain-text">
        <svg className="w-5 h-5 text-amber-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 10V3L4 14h7v7l9-11h-7z" />
        </svg>
        Brain Insights
      </h2>

      {/* Tabs */}
      <div className="flex gap-1 mb-3">
        {tabs.map((tab) => (
          <button key={tab.id} onClick={() => setActiveTab(tab.id)} className={`flex-1 text-xs font-mono py-1.5 rounded-lg transition-colors ${
            activeTab === tab.id ? "bg-amber-500/20 text-amber-400 border border-amber-500/30" : "text-brain-muted hover:text-brain-text border border-brain-border/30"
          }`}>{tab.label}</button>
        ))}
      </div>

      <div className="flex-1 overflow-y-auto space-y-3 min-h-0">
        {isLoading && <div className="text-center text-brain-muted text-sm font-mono py-4 animate-pulse">Analyzing brain...</div>}

        {activeTab === "overview" && trends && (
          <>
            {/* Brain IQ — Phase 2 scale 0-300 with 6 tiers */}
            {(() => {
              const iq = Math.round(trends.brain_iq);
              const tier = iq >= 250 ? { label: "Godlike", color: "text-fuchsia-400", glow: "shadow-fuchsia-500/40" }
                : iq >= 200 ? { label: "Superintelligent", color: "text-purple-400", glow: "shadow-purple-500/40" }
                : iq >= 150 ? { label: "Genius", color: "text-violet-400", glow: "shadow-violet-500/30" }
                : iq >= 100 ? { label: "Smart", color: "text-emerald-400", glow: "shadow-emerald-500/30" }
                : iq >= 50 ? { label: "Functional", color: "text-blue-400", glow: "shadow-blue-500/30" }
                : { label: "Basic", color: "text-amber-400", glow: "shadow-amber-500/30" };
              return (
                <div className={`text-center py-4 bg-brain-bg/50 border border-brain-border/30 rounded-lg shadow-lg ${tier.glow}`}>
                  <div className={`text-4xl font-mono font-bold ${tier.color}`}>{iq}</div>
                  <div className="text-[10px] text-brain-muted uppercase tracking-wider mt-0.5">Brain IQ - {tier.label}</div>
                  <div className="mt-2.5 mx-6 h-2 bg-brain-border/20 rounded-full overflow-hidden">
                    <div className="h-full rounded-full bg-gradient-to-r from-amber-500 via-blue-500 via-emerald-500 via-violet-500 to-fuchsia-500 transition-all duration-1000"
                      style={{ width: `${Math.min(100, (iq / 300) * 100)}%` }} />
                  </div>
                  <div className="flex justify-between mx-6 mt-1 text-[8px] text-brain-muted/50 font-mono">
                    <span>0</span><span>100</span><span>200</span><span>300</span>
                  </div>
                </div>
              );
            })()}

            {/* IQ Breakdown — three tiers */}
            {trends.iq_breakdown && (
              <section>
                <h3 className="text-xs font-semibold text-brain-muted uppercase tracking-wider mb-2">Foundation</h3>
                <IqBar label="Quality" value={trends.iq_breakdown.quality} max={25} color="bg-green-500" />
                <IqBar label="Connections" value={trends.iq_breakdown.connectivity} max={20} color="bg-blue-500" />
                <IqBar label="Freshness" value={trends.iq_breakdown.freshness} max={20} color="bg-cyan-500" />
                <IqBar label="Diversity" value={trends.iq_breakdown.diversity} max={15} color="bg-amber-500" />
                <IqBar label="Coverage" value={trends.iq_breakdown.coverage} max={10} color="bg-teal-500" />
                <IqBar label="Volume" value={trends.iq_breakdown.volume} max={10} color="bg-slate-400" />

                <h3 className="text-xs font-semibold text-brain-muted uppercase tracking-wider mb-2 mt-3">Intelligence</h3>
                <IqBar label="Depth" value={trends.iq_breakdown.depth} max={20} color="bg-purple-500" />
                <IqBar label="Cross-Domain" value={trends.iq_breakdown.cross_domain} max={20} color="bg-pink-500" />
                <IqBar label="Semantic" value={trends.iq_breakdown.semantic} max={20} color="bg-indigo-500" />
                <IqBar label="Research" value={trends.iq_breakdown.research_ratio} max={20} color="bg-violet-500" />
                <IqBar label="Coherence" value={trends.iq_breakdown.coherence} max={10} color="bg-fuchsia-500" />
                <IqBar label="HQ Ratio" value={trends.iq_breakdown.high_quality_pct} max={10} color="bg-emerald-500" />

                {/* Phase 2.4 — Meta-Intelligence tier */}
                <h3 className="text-xs font-semibold text-brain-muted uppercase tracking-wider mb-2 mt-3">
                  Meta-Intelligence
                  <span className="text-fuchsia-400/60 ml-1">★</span>
                </h3>
                <IqBar label="Self-Improve" value={trends.iq_breakdown.self_improvement_velocity ?? 0} max={25} color="bg-fuchsia-500" />
                <IqBar label="Predictions" value={trends.iq_breakdown.prediction_accuracy ?? 0} max={25} color="bg-rose-500" />
                <IqBar label="Insights/wk" value={trends.iq_breakdown.novel_insight_rate ?? 0} max={20} color="bg-pink-400" />
                <IqBar label="Independence" value={trends.iq_breakdown.autonomy_independence ?? 0} max={15} color="bg-purple-400" />
                <IqBar label="User Model" value={trends.iq_breakdown.user_model_accuracy ?? 0} max={15} color="bg-indigo-400" />
              </section>
            )}

            {/* Maximize IQ */}
            <button
              onClick={async () => {
                setBoostStatus("Maximizing IQ (research + quality sweep + cross-links)...");
                try {
                  const report = await maximizeIq();
                  setBoostStatus(report);
                  loadAll();
                } catch (err) {
                  setBoostStatus(`Error: ${err}`);
                }
              }}
              disabled={isLoading || boostStatus.includes("Maximizing")}
              className="w-full py-2.5 rounded-lg bg-gradient-to-r from-purple-500/20 via-pink-500/20 to-amber-500/20 text-purple-300 text-xs font-mono font-bold hover:from-purple-500/30 hover:via-pink-500/30 hover:to-amber-500/30 transition-all border border-purple-500/30 disabled:opacity-50 shadow-lg shadow-purple-500/10"
            >
              {boostStatus.includes("Maximizing") ? "Maximizing..." : boostStatus || "Maximize IQ"}
            </button>

            {/* Domain Growth */}
            <section>
              <h3 className="text-xs font-semibold text-brain-muted uppercase tracking-wider mb-2">Domain Growth</h3>
              {trends.domain_growth.map((d) => (
                <div key={d.domain} className="flex items-center gap-2 text-xs font-mono mb-1">
                  <div className="w-2 h-2 rounded-full" style={{ backgroundColor: getDomainColor(d.domain) }} />
                  <span className="text-brain-muted w-20">{d.domain}</span>
                  <div className="flex-1 h-2 bg-brain-border/20 rounded-full overflow-hidden">
                    <div className="h-full rounded-full" style={{ width: `${Math.min(100, d.count)}%`, backgroundColor: getDomainColor(d.domain) }} />
                  </div>
                  <span className="text-brain-text w-8 text-right">{d.count}</span>
                  {d.recent_count > 0 && <span className="text-green-400 text-[10px]">+{d.recent_count}</span>}
                </div>
              ))}
            </section>

            {/* Hot Topics */}
            <section>
              <h3 className="text-xs font-semibold text-brain-muted uppercase tracking-wider mb-2">Hot Topics</h3>
              {trends.hot_topics.slice(0, 5).map((t, i) => (
                <div key={i} className="flex items-center justify-between text-xs font-mono px-2 py-1">
                  <span className="text-brain-text">{t.topic}</span>
                  <span className="text-brain-muted/50">{t.node_count} nodes</span>
                </div>
              ))}
            </section>

            {/* Forgotten Topics */}
            {trends.forgotten_topics.length > 0 && (
              <section>
                <h3 className="text-xs font-semibold text-brain-muted uppercase tracking-wider mb-2">Forgotten Topics</h3>
                {trends.forgotten_topics.slice(0, 5).map((t, i) => (
                  <div key={i} className="flex items-center justify-between text-xs font-mono px-2 py-1 opacity-60">
                    <span className="text-brain-muted">{t.topic}</span>
                    <span className="text-red-400/50">{Math.round(t.score * 100)}% fresh</span>
                  </div>
                ))}
              </section>
            )}
          </>
        )}

        {activeTab === "patterns" && patterns && (
          <>
            {/* Hubs */}
            <section>
              <h3 className="text-xs font-semibold text-brain-muted uppercase tracking-wider mb-2">Hub Nodes (Most Connected)</h3>
              {patterns.hubs.slice(0, 5).map((h, i) => (
                <div key={i} className="flex items-center justify-between text-xs font-mono px-2 py-1">
                  <div className="flex items-center gap-2">
                    <div className="w-2 h-2 rounded-full" style={{ backgroundColor: getDomainColor(h.node.domain) }} />
                    <span className="text-brain-text">{h.node.title}</span>
                  </div>
                  <span className="text-brain-accent">{h.connection_count} links</span>
                </div>
              ))}
            </section>

            {/* Bridges */}
            {patterns.bridges.length > 0 && (
              <section>
                <h3 className="text-xs font-semibold text-brain-muted uppercase tracking-wider mb-2">Bridge Nodes (Cross-Domain)</h3>
                {patterns.bridges.slice(0, 5).map((b, i) => (
                  <div key={i} className="text-xs font-mono px-2 py-1">
                    <div className="text-brain-text">{b.node.title}</div>
                    <div className="text-[10px] text-brain-muted/50">Connects: {b.connects_domains.join(", ")}</div>
                  </div>
                ))}
              </section>
            )}

            {/* Islands */}
            {patterns.islands.length > 0 && (
              <section>
                <h3 className="text-xs font-semibold text-brain-muted uppercase tracking-wider mb-2">Islands ({patterns.islands.length} isolated)</h3>
                {patterns.islands.slice(0, 5).map((n, i) => (
                  <div key={i} className="text-xs font-mono px-2 py-1 text-brain-muted/60">
                    {n.title}
                  </div>
                ))}
              </section>
            )}

            {/* Clusters */}
            <section>
              <h3 className="text-xs font-semibold text-brain-muted uppercase tracking-wider mb-2">Clusters</h3>
              {patterns.clusters.map((c, i) => (
                <div key={i} className="flex items-center justify-between text-xs font-mono px-2 py-1">
                  <div className="flex items-center gap-2">
                    <div className="w-2 h-2 rounded-full" style={{ backgroundColor: getDomainColor(c.domain) }} />
                    <span className="text-brain-text">{c.domain}</span>
                  </div>
                  <span className="text-brain-muted">{c.node_count} nodes, {Math.round(c.avg_quality * 100)}% quality</span>
                </div>
              ))}
            </section>
          </>
        )}

        {activeTab === "recs" && recs.map((r, i) => (
          <div key={i} className="text-xs font-mono px-3 py-2 rounded-lg bg-brain-bg/50 border border-brain-border/30">
            <div className="flex items-center gap-2 mb-0.5">
              <span className={`text-[10px] px-1.5 py-0.5 rounded ${
                r.rec_type === "learn" ? "bg-purple-500/20 text-purple-400" :
                r.rec_type === "connect" ? "bg-blue-500/20 text-blue-400" :
                r.rec_type === "update" ? "bg-amber-500/20 text-amber-400" :
                "bg-green-500/20 text-green-400"
              }`}>{r.rec_type}</span>
              <span className="text-brain-text flex-1 truncate">{r.title}</span>
            </div>
            <div className="text-brain-muted/50 text-[10px]">{r.description}</div>
          </div>
        ))}
      </div>

      <button onClick={loadAll} disabled={isLoading} className="mt-3 w-full py-2 rounded-lg bg-amber-500/10 text-amber-400 text-xs font-mono hover:bg-amber-500/20 transition-colors border border-amber-500/20 disabled:opacity-50">
        {isLoading ? "Analyzing..." : "Refresh Insights"}
      </button>
    </div>
  );
}

function IqBar({ label, value, max, color }: { label: string; value: number; max: number; color: string }) {
  const pct = Math.min(100, (value / max) * 100);
  return (
    <div className="flex items-center gap-2 text-[10px] font-mono mb-1">
      <span className="text-brain-muted w-20 truncate">{label}</span>
      <div className="flex-1 h-1.5 bg-brain-border/20 rounded-full overflow-hidden">
        <div className={`h-full rounded-full ${color} transition-all duration-500`} style={{ width: `${pct}%` }} />
      </div>
      <span className="text-brain-text w-12 text-right">{value.toFixed(1)}/{max}</span>
    </div>
  );
}
