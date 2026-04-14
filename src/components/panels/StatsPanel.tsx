import { useState, useEffect } from "react";
import { getBrainStats, BrainStats, getDomainColor, autoLinkNodes } from "@/lib/tauri";
import { formatDate } from "@/lib/utils";
import { useGraphStore } from "@/stores/graphStore";

export function StatsPanel() {
  const cachedStats = useGraphStore((s) => s.stats);
  const [stats, setStats] = useState<BrainStats | null>(cachedStats);
  const [error, setError] = useState<string | null>(null);
  const [linking, setLinking] = useState(false);
  const [linkResult, setLinkResult] = useState<string | null>(null);

  const refreshStats = () => {
    setError(null);
    getBrainStats()
      .then(setStats)
      .catch((err) => {
        console.error(err);
        setError(String(err));
      });
  };

  useEffect(() => {
    refreshStats();

    // Auto-refresh every 5 minutes (was 30s — caused heavy DB load)
    const interval = setInterval(refreshStats, 300_000);
    return () => clearInterval(interval);
  }, []);

  const handleAutoLink = async () => {
    setLinking(true);
    setLinkResult(null);
    try {
      const result = await autoLinkNodes();
      setLinkResult(
        result.created > 0
          ? `Created ${result.created} new synapses`
          : "All neurons already linked"
      );
      // Refresh stats to show new synapse count
      refreshStats();
    } catch (err) {
      setLinkResult("Auto-link failed");
      console.error(err);
    } finally {
      setLinking(false);
    }
  };

  if (error) {
    return (
      <div className="p-4 text-center space-y-3">
        <div className="text-red-400 text-sm font-mono">Failed to load statistics</div>
        <div className="text-brain-muted text-xs font-mono break-all">{error}</div>
        <button
          onClick={refreshStats}
          className="px-4 py-2 rounded-lg text-xs font-mono border border-brain-accent/30 text-brain-accent hover:bg-brain-accent/10 transition-all"
        >
          Retry
        </button>
      </div>
    );
  }

  if (!stats) {
    return (
      <div className="p-4 text-center text-brain-muted text-sm">
        Loading statistics...
      </div>
    );
  }

  return (
    <div className="p-4 flex flex-col h-full">
      <h2 className="text-lg font-semibold mb-4 flex items-center gap-2">
        <svg className="w-5 h-5 text-brain-accent" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z" />
        </svg>
        Brain Statistics
      </h2>

      {/* Key metrics */}
      <div className="grid grid-cols-2 gap-3 mb-4">
        <StatCard label="Neurons" value={stats.total_nodes} color="#38BDF8" />
        <StatCard label="Synapses" value={stats.total_edges} color="#8B5CF6" />
        <StatCard label="Domains" value={stats.domains.length} color="#00CC88" />
        <StatCard label="Sources" value={stats.total_sources} color="#F59E0B" />
      </div>

      {/* Auto-link button */}
      <button
        onClick={handleAutoLink}
        disabled={linking}
        className="w-full mb-4 px-3 py-2 rounded-lg text-xs font-mono border transition-all
          border-brain-accent/30 text-brain-accent hover:bg-brain-accent/10
          disabled:opacity-50 disabled:cursor-not-allowed"
      >
        {linking ? "Linking neurons..." : "Auto-Link Neurons"}
      </button>
      {linkResult && (
        <div className="text-xs font-mono text-brain-muted mb-4 text-center">
          {linkResult}
        </div>
      )}

      {/* Domain breakdown */}
      <h3 className="text-sm font-semibold mb-3 text-brain-muted uppercase tracking-wider">
        Knowledge Domains
      </h3>
      <div className="space-y-2 mb-6">
        {stats.domains.map((d) => {
          const color = getDomainColor(d.domain);
          const maxCount = Math.max(...stats.domains.map((x) => x.count), 1);
          const width = (d.count / maxCount) * 100;

          return (
            <div key={d.domain} className="space-y-1">
              <div className="flex justify-between text-xs font-mono">
                <span style={{ color }}>{d.domain}</span>
                <span className="text-brain-muted">{d.count}</span>
              </div>
              <div className="h-1.5 bg-brain-bg/50 rounded-full overflow-hidden">
                <div
                  className="h-full rounded-full transition-all duration-500"
                  style={{ width: `${width}%`, backgroundColor: color }}
                />
              </div>
            </div>
          );
        })}
      </div>

      {/* Recent nodes */}
      <h3 className="text-sm font-semibold mb-3 text-brain-muted uppercase tracking-wider">
        Recent Knowledge
      </h3>
      <div className="flex-1 overflow-y-auto space-y-2">
        {stats.recent_nodes.map((node) => (
          <div
            key={node.id}
            className="p-2 rounded-lg bg-brain-bg/30 border border-brain-border/20 text-xs"
          >
            <div className="flex items-center gap-2 mb-1">
              <div
                className="w-2 h-2 rounded-full"
                style={{ backgroundColor: getDomainColor(node.domain) }}
              />
              <span className="text-brain-text font-medium truncate">
                {node.title}
              </span>
            </div>
            <span className="text-brain-muted font-mono">
              {formatDate(node.created_at)}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}

function StatCard({ label, value, color }: { label: string; value: number; color: string }) {
  return (
    <div className="p-3 rounded-lg bg-brain-bg/30 border border-brain-border/30">
      <div className="text-2xl font-bold font-mono" style={{ color }}>
        {value.toLocaleString()}
      </div>
      <div className="text-xs text-brain-muted uppercase tracking-wider mt-1">
        {label}
      </div>
    </div>
  );
}
