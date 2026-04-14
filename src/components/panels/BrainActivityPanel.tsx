import { useEffect, useState, useMemo } from "react";
import { getBrainActivity, BrainActivitySnapshot } from "@/lib/tauri";

/**
 * Phase 4.7 — "What's the brain doing right now" panel.
 *
 * Shows live circuit history, master loop status, memory tier passes,
 * and pending fine-tune runs. Polls every 5 seconds. Single Tauri call
 * fetches everything (`get_brain_activity`) so the polling cost stays
 * tiny.
 */
export function BrainActivityPanel() {
  const [snapshot, setSnapshot] = useState<BrainActivitySnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    async function tick() {
      try {
        const s = await getBrainActivity();
        if (!cancelled) {
          setSnapshot(s);
          setError(null);
        }
      } catch (e) {
        if (!cancelled) setError(String(e));
      }
    }

    tick();
    const interval = setInterval(tick, 5000);
    return () => {
      cancelled = true;
      clearInterval(interval);
    };
  }, []);

  // Dedup circuit entries — backend or event system may emit duplicates.
  // Key by circuit_name + started_at to discard exact repeats.
  const dedupedCircuits = useMemo(() => {
    if (!snapshot) return [];
    const seen = new Set<string>();
    return snapshot.recent_circuits.filter((c) => {
      const key = `${c.circuit_name}|${c.started_at}|${c.duration_ms}`;
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    });
  }, [snapshot]);

  if (error) {
    return (
      <div className="p-4 text-xs font-mono text-red-400">
        Failed to load brain activity: {error}
      </div>
    );
  }

  if (!snapshot) {
    return (
      <div className="p-4 flex items-center justify-center h-full">
        <span className="text-brain-muted text-sm font-mono animate-pulse">
          Loading brain activity...
        </span>
      </div>
    );
  }

  return (
    <div className="p-4 flex flex-col h-full">
      <h2 className="text-lg font-semibold mb-4 flex items-center gap-2 text-brain-text">
        <svg className="w-5 h-5 text-brain-accent" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 10V3L4 14h7v7l9-11h-7z" />
        </svg>
        Brain Activity
      </h2>

      <div className="text-[10px] text-brain-muted/60 font-mono mb-3">
        Refreshed every 5s · Last update {new Date(snapshot.generated_at).toLocaleTimeString()}
      </div>

      <div className="flex-1 overflow-y-auto space-y-4 min-h-0">
        {/* === Recent Circuit Runs === */}
        <section>
          <h3 className="text-xs font-semibold text-brain-muted uppercase tracking-wider mb-2">
            Recent Circuits ({dedupedCircuits.length})
          </h3>
          {dedupedCircuits.length === 0 ? (
            <div className="text-xs text-brain-muted/50 font-mono italic">
              No circuits have run yet. The first one fires after the autonomy
              loop's startup delay (~5 minutes).
            </div>
          ) : (
            <div className="space-y-1.5">
              {dedupedCircuits.slice(0, 12).map((c, i) => (
                <div
                  key={`${c.circuit_name}-${c.started_at}`}
                  className={`text-[10px] font-mono px-2 py-1.5 rounded border ${
                    c.status === "ok"
                      ? "bg-emerald-500/5 border-emerald-500/20"
                      : "bg-red-500/5 border-red-500/20"
                  }`}
                >
                  <div className="flex items-center justify-between mb-0.5">
                    <span
                      className={
                        c.status === "ok" ? "text-emerald-400 font-bold" : "text-red-400 font-bold"
                      }
                    >
                      {c.circuit_name}
                    </span>
                    <span className="text-brain-muted/50">
                      {(c.duration_ms / 1000).toFixed(1)}s · {timeAgo(c.started_at)}
                    </span>
                  </div>
                  <div className="text-brain-muted truncate">{c.result}</div>
                </div>
              ))}
            </div>
          )}
        </section>

        {/* === Circuit Health === */}
        {snapshot.circuit_health.length > 0 && (
          <section>
            <h3 className="text-xs font-semibold text-brain-muted uppercase tracking-wider mb-2">
              Circuit Health
            </h3>
            <div className="space-y-1">
              {snapshot.circuit_health.map((h) => {
                const total = h.success_count + h.fail_count;
                const success_pct = total > 0 ? (h.success_count / total) * 100 : 0;
                return (
                  <div key={h.circuit_name} className="flex items-center gap-2 text-[10px] font-mono">
                    <span className="text-brain-text w-32 truncate">{h.circuit_name}</span>
                    <div className="flex-1 h-1.5 bg-brain-border/20 rounded-full overflow-hidden">
                      <div
                        className={`h-full rounded-full ${
                          success_pct >= 80 ? "bg-emerald-500" : success_pct >= 50 ? "bg-amber-500" : "bg-red-500"
                        }`}
                        style={{ width: `${success_pct}%` }}
                      />
                    </div>
                    <span className="text-brain-muted w-16 text-right">
                      {h.success_count}✓ {h.fail_count > 0 && <span className="text-red-400">{h.fail_count}✗</span>}
                    </span>
                    <span className="text-brain-muted/50 w-10 text-right">{(h.avg_duration_ms / 1000).toFixed(1)}s</span>
                  </div>
                );
              })}
            </div>
          </section>
        )}

        {/* === Master Cognitive Loop === */}
        <section>
          <h3 className="text-xs font-semibold text-brain-muted uppercase tracking-wider mb-2">
            Master Cognitive Loop
          </h3>
          {snapshot.recent_master_loops.length === 0 ? (
            <div className="text-xs text-brain-muted/50 font-mono italic">
              Master loop hasn't fired yet. First cycle ~10 minutes after startup.
            </div>
          ) : (
            <div className="space-y-1.5">
              {snapshot.recent_master_loops.slice(0, 5).map((m, i) => (
                <div key={`ml-${m.started_at}`} className="text-[10px] font-mono px-2 py-1.5 rounded bg-brain-bg/50 border border-brain-border/30">
                  <div className="flex items-center justify-between mb-0.5">
                    <span className={
                      m.health === "growing" ? "text-emerald-400 font-bold"
                      : m.health === "healthy" ? "text-cyan-400 font-bold"
                      : m.health === "degraded" ? "text-amber-400 font-bold"
                      : "text-red-400 font-bold"
                    }>{m.health}</span>
                    <span className="text-brain-muted/50">{timeAgo(m.started_at)}</span>
                  </div>
                  <div className="text-brain-muted">
                    +{m.new_nodes_24h} nodes / +{m.new_thinking_nodes_24h} thinking ({(m.thinking_ratio * 100).toFixed(0)}%)
                    {m.missions_queued > 0 && <span className="text-purple-400 ml-2">· {m.missions_queued} missions queued</span>}
                  </div>
                </div>
              ))}
            </div>
          )}
        </section>

        {/* === Memory Tier === */}
        <section>
          <h3 className="text-xs font-semibold text-brain-muted uppercase tracking-wider mb-2">
            Memory Tier Promotion
          </h3>
          {snapshot.recent_memory_tier_passes.length === 0 ? (
            <div className="text-xs text-brain-muted/50 font-mono italic">
              No tier promotion passes yet (runs every 6 hours after a 15-min warm-up).
            </div>
          ) : (
            <div className="space-y-1">
              {snapshot.recent_memory_tier_passes.slice(0, 3).map((t, i) => (
                <div key={`mt-${t.ran_at}`} className="text-[10px] font-mono px-2 py-1.5 rounded bg-brain-bg/50 border border-brain-border/30 flex items-center justify-between">
                  <div className="flex gap-2">
                    <span className="text-rose-400">🔥{t.promoted_hot}</span>
                    <span className="text-amber-400">☀{t.promoted_warm}</span>
                    <span className="text-blue-400">❄{t.demoted_cold}</span>
                    <span className="text-brain-muted/50">·</span>
                    <span className="text-brain-muted">{t.scanned} scanned</span>
                  </div>
                  <span className="text-brain-muted/50">{timeAgo(t.ran_at)}</span>
                </div>
              ))}
            </div>
          )}
        </section>

        {/* === Pending Fine-tunes === */}
        {snapshot.pending_fine_tunes.length > 0 && (
          <section>
            <h3 className="text-xs font-semibold text-brain-muted uppercase tracking-wider mb-2">
              Fine-Tune Runs
            </h3>
            <div className="space-y-1">
              {snapshot.pending_fine_tunes.slice(0, 3).map((f, i) => (
                <div key={`ft-${f.timestamp}-${f.status}`} className="text-[10px] font-mono px-2 py-1.5 rounded bg-purple-500/10 border border-purple-500/30">
                  <div className="flex items-center justify-between">
                    <span className="text-purple-400 font-bold">{f.timestamp}</span>
                    <span className={
                      f.status === "completed" ? "text-emerald-400"
                      : f.status === "prepared" ? "text-amber-400"
                      : "text-brain-muted"
                    }>{f.status}</span>
                  </div>
                  <div className="text-brain-muted mt-0.5">
                    {f.dataset_entries} entries · {(f.dataset_size_bytes / 1024).toFixed(0)} KB
                  </div>
                  <div className="text-brain-muted/50 mt-0.5 truncate text-[9px]">
                    {f.script_path}
                  </div>
                </div>
              ))}
            </div>
          </section>
        )}
      </div>
    </div>
  );
}

function timeAgo(iso: string): string {
  try {
    const then = new Date(iso).getTime();
    const now = Date.now();
    const diff = Math.floor((now - then) / 1000);
    if (diff < 60) return `${diff}s ago`;
    if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
    if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
    return `${Math.floor(diff / 86400)}d ago`;
  } catch {
    return iso;
  }
}
