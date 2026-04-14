import { useState, useEffect } from "react";
import {
  getQualityReport,
  calculateQuality,
  calculateDecay,
  getEmbeddingStats,
  enhanceQuality,
  QualityReport,
  EmbeddingStats,
} from "@/lib/tauri";

export function QualityPanel() {
  const [report, setReport] = useState<QualityReport | null>(null);
  const [embStats, setEmbStats] = useState<EmbeddingStats | null>(null);
  const [status, setStatus] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [isProcessing, setIsProcessing] = useState(false);

  useEffect(() => {
    loadReport();
  }, []);

  const loadReport = async () => {
    setError(null);
    setLoading(true);
    try {
      const [r, e] = await Promise.all([
        getQualityReport().catch((err) => { console.error("Quality report failed:", err); return null; }),
        getEmbeddingStats().catch((err) => { console.error("Embedding stats failed:", err); return null; }),
      ]);
      setReport(r);
      setEmbStats(e);
      if (!r && !e) {
        setError("Could not load brain health data. The database may be busy.");
      }
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  };

  const handleRecalculate = async () => {
    setIsProcessing(true);
    setStatus("Recalculating quality scores...");
    try {
      const [qUpdated] = await calculateQuality();
      setStatus(`Quality: ${qUpdated} nodes scored. Calculating decay...`);
      const [dUpdated] = await calculateDecay();
      setStatus(`Done! ${qUpdated} quality + ${dUpdated} decay scores updated`);
      loadReport();
    } catch (err) {
      setStatus(`Error: ${err}`);
    } finally {
      setIsProcessing(false);
    }
  };

  return (
    <div className="p-4 flex flex-col h-full">
      <h2 className="text-lg font-semibold mb-4 flex items-center gap-2 text-brain-text">
        <svg className="w-5 h-5 text-green-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" />
        </svg>
        Brain Health
      </h2>

      {status && (
        <div className="mb-3 text-xs font-mono px-3 py-2 rounded-lg bg-green-500/10 text-green-400 border border-green-500/20">
          {status}
        </div>
      )}

      {loading && !report && (
        <div className="text-center text-brain-muted text-sm py-4">Loading brain health...</div>
      )}

      {error && (
        <div className="mb-3 space-y-2">
          <div className="text-xs font-mono px-3 py-2 rounded-lg bg-red-500/10 text-red-400 border border-red-500/20">
            {error}
          </div>
          <button
            onClick={loadReport}
            className="w-full py-2 rounded-lg text-xs font-mono border border-brain-accent/30 text-brain-accent hover:bg-brain-accent/10 transition-all"
          >
            Retry
          </button>
        </div>
      )}

      <div className="flex-1 overflow-y-auto space-y-4 min-h-0">
        {/* Quality Metrics */}
        {report && (
          <div className="grid grid-cols-2 gap-2">
            <MetricCard label="Avg Quality" value={`${Math.round(report.avg_quality * 100)}%`} color="text-green-400" />
            <MetricCard label="Avg Freshness" value={`${Math.round(report.avg_decay * 100)}%`} color="text-blue-400" />
            <MetricCard label="High Quality" value={`${report.high_quality_count}`} color="text-emerald-400" />
            <MetricCard label="Low Quality" value={`${report.low_quality_count}`} color="text-red-400" />
            <MetricCard label="Decayed" value={`${report.decayed_count}`} color="text-amber-400" />
            <MetricCard label="Total Nodes" value={`${report.total_nodes}`} color="text-brain-accent" />
          </div>
        )}

        {/* Embedding Status */}
        {embStats && (
          <section>
            <h3 className="text-xs font-semibold text-brain-muted uppercase tracking-wider mb-2">
              Embeddings
            </h3>
            <div className="space-y-1.5">
              <div className="flex items-center justify-between text-xs font-mono">
                <span className="text-brain-muted">Ollama</span>
                <span className={embStats.ollama_connected ? "text-green-400" : "text-red-400"}>
                  {embStats.ollama_connected ? "Connected" : "Disconnected"}
                </span>
              </div>
              <div className="flex items-center justify-between text-xs font-mono">
                <span className="text-brain-muted">Embedded</span>
                <span className="text-brain-text">{embStats.embedded_nodes} / {embStats.total_nodes}</span>
              </div>
              {embStats.total_nodes > 0 && (
                <div className="w-full h-2 rounded-full bg-brain-border/30 overflow-hidden">
                  <div
                    className="h-full rounded-full bg-gradient-to-r from-brain-accent to-brain-research transition-all"
                    style={{ width: `${(embStats.embedded_nodes / embStats.total_nodes) * 100}%` }}
                  />
                </div>
              )}
              <div className="text-[10px] text-brain-muted/50 font-mono">
                {embStats.pending_nodes} nodes pending embedding
              </div>
            </div>
          </section>
        )}

        {/* Quality Distribution Visual */}
        {report && report.total_nodes > 0 && (
          <section>
            <h3 className="text-xs font-semibold text-brain-muted uppercase tracking-wider mb-2">
              Quality Distribution
            </h3>
            <div className="flex h-6 rounded-lg overflow-hidden border border-brain-border/30">
              <div className="bg-red-500/40 transition-all" style={{ width: `${(report.low_quality_count / report.total_nodes) * 100}%` }} title="Low quality" />
              <div className="bg-amber-500/40 transition-all flex-1" title="Medium quality" />
              <div className="bg-green-500/40 transition-all" style={{ width: `${(report.high_quality_count / report.total_nodes) * 100}%` }} title="High quality" />
            </div>
            <div className="flex justify-between text-[10px] font-mono text-brain-muted/50 mt-1">
              <span>Low ({report.low_quality_count})</span>
              <span>Medium</span>
              <span>High ({report.high_quality_count})</span>
            </div>
          </section>
        )}
      </div>

      {/* Actions */}
      <div className="mt-3 space-y-2">
        <button
          onClick={handleRecalculate}
          disabled={isProcessing}
          className="w-full py-2 rounded-lg bg-gradient-to-r from-green-500/20 to-blue-500/20 text-brain-text text-xs font-mono hover:from-green-500/30 hover:to-blue-500/30 transition-all disabled:opacity-50 border border-green-500/20"
        >
          {isProcessing ? "Processing..." : "Recalculate Quality & Decay"}
        </button>
        <button
          onClick={async () => {
            setIsProcessing(true);
            setStatus("Enhancing brain quality with AI (summaries + tags)...");
            try {
              const [summaries, tags, failed] = await enhanceQuality();
              setStatus(`Enhanced! ${summaries} summaries + ${tags} tags generated (${failed} failed)`);
              loadReport();
            } catch (err) {
              setStatus(`Error: ${err}`);
            } finally {
              setIsProcessing(false);
            }
          }}
          disabled={isProcessing}
          className="w-full py-2 rounded-lg bg-gradient-to-r from-purple-500/20 to-pink-500/20 text-brain-text text-xs font-mono hover:from-purple-500/30 hover:to-pink-500/30 transition-all disabled:opacity-50 border border-purple-500/20"
        >
          {isProcessing ? "Enhancing..." : "Enhance Brain Quality (AI)"}
        </button>
      </div>
    </div>
  );
}

function MetricCard({ label, value, color }: { label: string; value: string; color: string }) {
  return (
    <div className="bg-brain-bg/50 border border-brain-border/30 rounded-lg px-3 py-2 text-center">
      <div className={`text-lg font-mono font-bold ${color}`}>{value}</div>
      <div className="text-[10px] text-brain-muted uppercase tracking-wider">{label}</div>
    </div>
  );
}
