import { useSidekickStore, type ActivityItem } from "@/stores/sidekickStore";

const SEVERITY_COLORS: Record<string, string> = {
  info: "border-l-blue-500",
  success: "border-l-green-500",
  warning: "border-l-amber-500",
  error: "border-l-red-500",
};

const TYPE_ICONS: Record<string, string> = {
  node: "N",
  edge: "S",
  ingestion: "I",
  research: "R",
  autonomy: "A",
  learning: "L",
  export: "E",
  system: "S",
};

export function ActivityFeed() {
  const feed = useSidekickStore((s) => s.activityFeed);
  const expanded = useSidekickStore((s) => s.feedExpanded);
  const unreadCount = useSidekickStore((s) => s.unreadCount);
  const toggleExpanded = useSidekickStore((s) => s.toggleFeedExpanded);

  const latest = feed[0];

  return (
    <div className="relative">
      {/* Collapsed: single-line ticker */}
      {!expanded && (
        <button
          onClick={toggleExpanded}
          className="glass-panel px-3 py-1.5 flex items-center gap-2 text-xs font-mono text-brain-muted hover:text-brain-text transition-colors w-full text-left"
        >
          <div className="w-1.5 h-1.5 rounded-full bg-brain-accent animate-pulse" />
          {latest ? (
            <span className="truncate flex-1">{latest.message}</span>
          ) : (
            <span className="text-brain-muted/50">Brain activity feed</span>
          )}
          {unreadCount > 0 && (
            <span className="bg-brain-accent/20 text-brain-accent px-1.5 py-0.5 rounded-full text-[10px]">
              {unreadCount}
            </span>
          )}
        </button>
      )}

      {/* Expanded: scrollable feed */}
      {expanded && (
        <div className="glass-panel w-[340px] max-h-[300px] overflow-hidden flex flex-col">
          {/* Header */}
          <div className="px-3 py-2 flex items-center justify-between border-b border-brain-border/30">
            <span className="text-xs font-mono text-brain-accent font-semibold">Brain Activity</span>
            <button
              onClick={toggleExpanded}
              className="text-brain-muted hover:text-brain-text text-xs"
            >
              Collapse
            </button>
          </div>

          {/* Feed items */}
          <div className="overflow-y-auto flex-1 max-h-[260px]">
            {feed.length === 0 ? (
              <div className="p-4 text-center text-brain-muted text-xs">
                No activity yet
              </div>
            ) : (
              feed.slice(0, 30).map((item) => (
                <ActivityRow key={item.id} item={item} />
              ))
            )}
          </div>
        </div>
      )}
    </div>
  );
}

function ActivityRow({ item }: { item: ActivityItem }) {
  const elapsed = formatElapsed(Date.now() - item.timestamp);
  const borderColor = SEVERITY_COLORS[item.severity] || SEVERITY_COLORS.info;

  return (
    <div className={`px-3 py-2 border-l-2 ${borderColor} border-b border-brain-border/10 hover:bg-brain-bg/30`}>
      <div className="flex items-start gap-2">
        <span className="text-[9px] font-mono text-brain-muted bg-brain-bg/50 px-1 rounded mt-0.5">
          {TYPE_ICONS[item.type] || "?"}
        </span>
        <div className="flex-1 min-w-0">
          <p className="text-xs font-mono text-brain-text truncate">{item.message}</p>
          {item.detail && (
            <p className="text-[10px] font-mono text-brain-muted truncate">{item.detail}</p>
          )}
        </div>
        <span className="text-[10px] font-mono text-brain-muted/50 whitespace-nowrap">{elapsed}</span>
      </div>
    </div>
  );
}

function formatElapsed(ms: number): string {
  const seconds = Math.floor(ms / 1000);
  if (seconds < 5) return "now";
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  return `${hours}h`;
}
