import { useState, useEffect, useRef } from "react";
import {
  getAutonomyStatus,
  setAutonomyEnabled,
  triggerAutonomyTask,
  onBrainEvent,
  AutonomyStatus,
  BrainEventPayload,
} from "@/lib/tauri";

interface ActivityEvent {
  id: number;
  time: string;
  message: string;
  type: "info" | "success" | "error";
}

const TASK_LABELS: Record<string, string> = {
  auto_link: "Auto-Link Neurons",
  quality_recalc: "Quality & Decay Recalc",
  quality_sweep: "AI Quality Sweep",
  iq_boost: "Cross-Domain IQ Boost",
  active_learning: "Active Learning",
  export: "Export & Briefing",
};

export function AutonomyPanel() {
  const [status, setStatus] = useState<AutonomyStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [triggeringTask, setTriggeringTask] = useState<string | null>(null);
  const [activity, setActivity] = useState<ActivityEvent[]>([]);
  const nextId = useRef(0);

  const refresh = () => {
    getAutonomyStatus()
      .then(setStatus)
      .catch((err) => setError(String(err)));
  };

  useEffect(() => {
    refresh();
    const interval = setInterval(refresh, 15_000);

    // Listen for autonomy events
    let unlisten: (() => void) | null = null;
    onBrainEvent((event: BrainEventPayload) => {
      const { type } = event;
      if (type?.startsWith("Autonomy") || type === "ActiveLearningCompleted" || type === "BriefingUpdated") {
        const id = nextId.current++;
        let message = "";
        let kind: "info" | "success" | "error" = "info";

        if (type === "AutonomyTaskStarted") {
          message = `Started: ${TASK_LABELS[event.task || ""] || event.task}`;
        } else if (type === "AutonomyTaskCompleted") {
          message = `Completed: ${TASK_LABELS[event.task || ""] || event.task} (${event.duration_ms}ms) — ${event.result}`;
          kind = "success";
        } else if (type === "AutonomyTaskFailed") {
          message = `Failed: ${TASK_LABELS[event.task || ""] || event.task} — ${event.error}`;
          kind = "error";
        } else if (type === "ActiveLearningCompleted") {
          message = `Learned ${event.topics_researched ?? 0} topics → ${event.nodes_created ?? 0} nodes`;
          kind = "success";
        } else if (type === "BriefingUpdated") {
          message = `Briefing updated: IQ ${Math.round(event.iq ?? 0)}/200, ${event.total_nodes ?? 0} neurons`;
          kind = "success";
        }

        if (message) {
          const time = new Date().toLocaleTimeString();
          setActivity((prev) => [{ id, time, message, type: kind } as ActivityEvent, ...prev].slice(0, 50));
        }
        refresh();
      }
    }).then((fn) => { unlisten = fn; });

    return () => {
      clearInterval(interval);
      unlisten?.();
    };
  }, []);

  const handleToggle = async () => {
    if (!status) return;
    try {
      await setAutonomyEnabled(!status.enabled);
      refresh();
    } catch (err) {
      setError(String(err));
    }
  };

  const handleTrigger = async (task: string) => {
    setTriggeringTask(task);
    try {
      const result = await triggerAutonomyTask(task);
      const id = nextId.current++;
      setActivity((prev) => [
        { id, time: new Date().toLocaleTimeString(), message: `Manual: ${result}`, type: "success" } as ActivityEvent,
        ...prev,
      ].slice(0, 50));
      refresh();
    } catch (err) {
      setError(String(err));
    } finally {
      setTriggeringTask(null);
    }
  };

  const formatTime = (iso: string | null) => {
    if (!iso) return "Never";
    try {
      const d = new Date(iso);
      const now = new Date();
      const diffMs = now.getTime() - d.getTime();
      const mins = Math.floor(diffMs / 60000);
      if (mins < 1) return "Just now";
      if (mins < 60) return `${mins}m ago`;
      const hours = Math.floor(mins / 60);
      if (hours < 24) return `${hours}h ago`;
      return `${Math.floor(hours / 24)}d ago`;
    } catch {
      return iso;
    }
  };

  return (
    <div className="p-4 flex flex-col h-full">
      <h2 className="text-lg font-semibold mb-4 flex items-center gap-2 text-brain-text">
        <svg className="w-5 h-5 text-emerald-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
        </svg>
        Brain Autonomy
      </h2>

      {error && (
        <div className="mb-3 text-xs font-mono px-3 py-2 rounded-lg bg-red-500/10 text-red-400 border border-red-500/20">
          {error}
          <button onClick={() => setError(null)} className="ml-2 underline">dismiss</button>
        </div>
      )}

      {/* Enable/Disable Toggle */}
      <div className="flex items-center justify-between mb-4 px-3 py-2 rounded-lg bg-brain-bg/50 border border-brain-border/30">
        <div>
          <div className="text-sm font-medium text-brain-text">Self-Improving Mode</div>
          <div className="text-[10px] text-brain-muted">Brain actively learns, links, and improves</div>
        </div>
        <button
          onClick={handleToggle}
          className={`w-12 h-6 rounded-full transition-all relative ${
            status?.enabled ? "bg-emerald-500" : "bg-brain-border/50"
          }`}
        >
          <div className={`w-5 h-5 rounded-full bg-white absolute top-0.5 transition-all ${
            status?.enabled ? "left-6" : "left-0.5"
          }`} />
        </button>
      </div>

      {/* Today's Stats */}
      {status && (
        <div className="grid grid-cols-2 gap-2 mb-4">
          <MiniStat label="Topics Learned" value={status.today.topics_researched} color="text-brain-research" />
          <MiniStat label="Links Made" value={status.today.links_made} color="text-brain-accent" />
        </div>
      )}

      {/* Task Schedule */}
      <h3 className="text-xs font-semibold text-brain-muted uppercase tracking-wider mb-2">
        Task Schedule
      </h3>
      <div className="space-y-1.5 mb-4">
        {status?.tasks.map((task) => (
          <div
            key={task.name}
            className="flex items-center justify-between px-2 py-1.5 rounded-lg bg-brain-bg/30 border border-brain-border/20 text-xs"
          >
            <div className="flex-1 min-w-0">
              <div className="font-mono text-brain-text truncate">
                {TASK_LABELS[task.name] || task.name}
              </div>
              <div className="text-[10px] text-brain-muted truncate">
                {task.last_result && !task.last_result.startsWith("FAILED")
                  ? task.last_result
                  : formatTime(task.last_run)
                }
              </div>
            </div>
            <div className="flex items-center gap-1.5 ml-2">
              {task.runs_today > 0 && (
                <span className="text-[10px] font-mono text-brain-muted bg-brain-border/20 px-1 rounded">
                  {task.runs_today}x
                </span>
              )}
              <button
                onClick={() => handleTrigger(task.name)}
                disabled={triggeringTask === task.name || !status.enabled}
                className="px-1.5 py-0.5 rounded text-[10px] font-mono border border-brain-accent/30 text-brain-accent hover:bg-brain-accent/10 disabled:opacity-30 transition-all"
                title="Run now"
              >
                {triggeringTask === task.name ? "..." : "Run"}
              </button>
            </div>
          </div>
        ))}
      </div>

      {/* Activity Feed */}
      <h3 className="text-xs font-semibold text-brain-muted uppercase tracking-wider mb-2">
        Activity Feed
      </h3>
      <div className="flex-1 overflow-y-auto space-y-1 min-h-0">
        {activity.length === 0 ? (
          <div className="text-center text-brain-muted text-xs py-4">
            {status?.enabled ? "Waiting for next autonomy cycle..." : "Autonomy is disabled"}
          </div>
        ) : (
          activity.map((evt) => (
            <div
              key={evt.id}
              className={`text-[11px] font-mono px-2 py-1 rounded border ${
                evt.type === "success"
                  ? "bg-green-500/5 border-green-500/20 text-green-400"
                  : evt.type === "error"
                  ? "bg-red-500/5 border-red-500/20 text-red-400"
                  : "bg-brain-bg/30 border-brain-border/20 text-brain-muted"
              }`}
            >
              <span className="text-brain-muted/50">{evt.time}</span>{" "}
              {evt.message}
            </div>
          ))
        )}
      </div>
    </div>
  );
}

function MiniStat({ label, value, color }: { label: string; value: number; color: string }) {
  return (
    <div className="bg-brain-bg/50 border border-brain-border/30 rounded-lg px-2 py-1.5 text-center">
      <div className={`text-lg font-mono font-bold ${color}`}>{value}</div>
      <div className="text-[10px] text-brain-muted uppercase tracking-wider">{label}</div>
    </div>
  );
}
