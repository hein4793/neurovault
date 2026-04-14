import { useUiStore } from "@/stores/uiStore";

export function ViewModeSelector() {
  const { viewMode, setViewMode, heatmapMetric, setHeatmapMetric } = useUiStore();

  return (
    <div className="glass-panel px-3 py-2 flex items-center gap-2">
      {(["default", "heatmap", "cluster"] as const).map((mode) => (
        <button
          key={mode}
          onClick={() => setViewMode(mode)}
          className={`text-[10px] px-2 py-1 rounded font-mono transition-colors ${
            viewMode === mode
              ? "bg-brain-accent/20 text-brain-accent"
              : "text-brain-muted hover:text-brain-text"
          }`}
        >
          {mode}
        </button>
      ))}
      {viewMode === "heatmap" && (
        <select
          value={heatmapMetric}
          onChange={(e) => setHeatmapMetric(e.target.value as any)}
          className="text-[10px] bg-brain-bg/50 border border-brain-border/30 rounded px-1 py-0.5 text-brain-muted font-mono outline-none"
        >
          <option value="quality">Quality</option>
          <option value="decay">Freshness</option>
          <option value="access">Activity</option>
          <option value="connections">Connections</option>
        </select>
      )}
    </div>
  );
}
