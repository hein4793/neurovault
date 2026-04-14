import { useState, useEffect } from "react";
import {
  getSwarmStatus,
  getSwarmTasks,
  decomposeGoal,
  SwarmStatus,
  SwarmTask,
  GoalDecomposition,
} from "@/lib/tauri";

const STATUS_STYLES: Record<string, string> = {
  idle: "bg-brain-border/20 text-brain-muted",
  active: "bg-green-500/20 text-green-400",
  busy: "bg-amber-500/20 text-amber-400",
  error: "bg-red-500/20 text-red-400",
  pending: "bg-amber-500/20 text-amber-400",
  running: "bg-blue-500/20 text-blue-400",
  completed: "bg-green-500/20 text-green-400",
  failed: "bg-red-500/20 text-red-400",
};

export function SwarmPanel() {
  const [status, setStatus] = useState<SwarmStatus | null>(null);
  const [tasks, setTasks] = useState<SwarmTask[]>([]);
  const [decomposition, setDecomposition] = useState<GoalDecomposition | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [isDecomposing, setIsDecomposing] = useState(false);
  const [goalInput, setGoalInput] = useState("");
  const [activeTab, setActiveTab] = useState<"agents" | "tasks" | "goals">("agents");

  useEffect(() => {
    loadAll();
  }, []);

  const loadAll = async () => {
    setIsLoading(true);
    try {
      const s = await getSwarmStatus().catch(() => null);
      setStatus(s);
      const t = await getSwarmTasks().catch(() => []);
      setTasks(t);
    } catch (err) {
      setError(String(err));
    } finally {
      setIsLoading(false);
    }
  };

  const handleDecompose = async () => {
    if (!goalInput.trim()) return;
    setIsDecomposing(true);
    setError(null);
    try {
      const result = await decomposeGoal(goalInput.trim());
      setDecomposition(result);
      loadAll();
    } catch (err) {
      setError(String(err));
    } finally {
      setIsDecomposing(false);
    }
  };

  const tabs = [
    { id: "agents" as const, label: "Agents" },
    { id: "tasks" as const, label: `Tasks (${tasks.length})` },
    { id: "goals" as const, label: "Goals" },
  ];

  return (
    <div className="p-4 flex flex-col h-full">
      <h2 className="text-lg font-semibold mb-1 flex items-center gap-2 text-brain-text">
        <svg className="w-5 h-5 text-orange-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M17 20h5v-2a3 3 0 00-5.356-1.857M17 20H7m10 0v-2c0-.656-.126-1.283-.356-1.857M7 20H2v-2a3 3 0 015.356-1.857M7 20v-2c0-.656.126-1.283.356-1.857m0 0a5.002 5.002 0 019.288 0M15 7a3 3 0 11-6 0 3 3 0 016 0zm6 3a2 2 0 11-4 0 2 2 0 014 0zM7 10a2 2 0 11-4 0 2 2 0 014 0z" />
        </svg>
        Agent Swarm
      </h2>
      <p className="text-[10px] text-brain-muted mb-3">Specialist agents, task execution, goal decomposition</p>

      {error && (
        <div className="mb-3 text-xs font-mono px-3 py-2 rounded-lg bg-red-500/10 text-red-400 border border-red-500/20">
          {error}
          <button onClick={() => setError(null)} className="ml-2 underline">dismiss</button>
        </div>
      )}

      {/* Summary Stats */}
      {status && (
        <div className="grid grid-cols-3 gap-2 mb-3">
          <div className="bg-brain-bg/50 border border-brain-border/30 rounded-lg px-2 py-1.5 text-center">
            <div className="text-lg font-mono font-bold text-orange-400">{status.agents.length}</div>
            <div className="text-[10px] text-brain-muted uppercase tracking-wider">Agents</div>
          </div>
          <div className="bg-brain-bg/50 border border-brain-border/30 rounded-lg px-2 py-1.5 text-center">
            <div className="text-lg font-mono font-bold text-blue-400">{status.active_tasks}</div>
            <div className="text-[10px] text-brain-muted uppercase tracking-wider">Active</div>
          </div>
          <div className="bg-brain-bg/50 border border-brain-border/30 rounded-lg px-2 py-1.5 text-center">
            <div className="text-lg font-mono font-bold text-green-400">{status.completed_tasks}</div>
            <div className="text-[10px] text-brain-muted uppercase tracking-wider">Done</div>
          </div>
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
                ? "bg-orange-500/20 text-orange-400 border border-orange-500/30"
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
            Loading swarm...
          </div>
        )}

        {/* Agents Tab */}
        {activeTab === "agents" && status && (
          <>
            {status.agents.map((agent) => (
              <div
                key={agent.name}
                className="bg-brain-bg/50 border border-brain-border/30 rounded-lg px-3 py-2"
              >
                <div className="flex items-center justify-between mb-1">
                  <span className="text-xs font-mono font-semibold text-brain-text">{agent.name}</span>
                  <span className={`text-[10px] px-1.5 py-0.5 rounded font-mono ${
                    STATUS_STYLES[agent.status] || STATUS_STYLES.idle
                  }`}>
                    {agent.status}
                  </span>
                </div>
                <div className="flex items-center gap-2 mb-1.5">
                  <span className="text-[10px] text-brain-muted">Autonomy:</span>
                  <div className="flex-1 h-1.5 bg-brain-border/20 rounded-full overflow-hidden">
                    <div
                      className="h-full rounded-full bg-orange-500 transition-all"
                      style={{ width: `${Math.min(100, agent.autonomy_level * 100)}%` }}
                    />
                  </div>
                  <span className="text-[10px] font-mono text-brain-muted">
                    {(agent.autonomy_level * 100).toFixed(0)}%
                  </span>
                </div>
                <div className="flex flex-wrap gap-1">
                  {agent.capabilities.map((cap) => (
                    <span
                      key={cap}
                      className="text-[9px] px-1.5 py-0.5 rounded bg-orange-500/10 text-orange-400/80 border border-orange-500/20 font-mono"
                    >
                      {cap}
                    </span>
                  ))}
                </div>
                {agent.current_task && (
                  <div className="mt-1.5 text-[10px] text-brain-muted/70 font-mono truncate">
                    Working on: {agent.current_task}
                  </div>
                )}
              </div>
            ))}
            {status.agents.length === 0 && (
              <div className="text-center text-brain-muted text-xs py-4">No agents registered</div>
            )}
          </>
        )}

        {/* Tasks Tab */}
        {activeTab === "tasks" && (
          <>
            {tasks.map((task) => (
              <div
                key={task.id}
                className="bg-brain-bg/50 border border-brain-border/30 rounded-lg px-3 py-2"
              >
                <div className="flex items-center justify-between mb-0.5">
                  <span className="text-xs text-brain-text truncate flex-1">{task.description}</span>
                  <span className={`text-[10px] px-1.5 py-0.5 rounded font-mono ml-2 flex-shrink-0 ${
                    STATUS_STYLES[task.status] || STATUS_STYLES.pending
                  }`}>
                    {task.status}
                  </span>
                </div>
                <div className="flex items-center gap-3 text-[10px] font-mono text-brain-muted/60">
                  {task.assigned_agent && <span>Agent: {task.assigned_agent}</span>}
                  <span>Priority: {task.priority}</span>
                  <span className="font-mono">{task.id.slice(0, 8)}</span>
                </div>
                {task.result && (
                  <div className="mt-1 text-[10px] text-brain-muted/70 truncate">
                    Result: {task.result}
                  </div>
                )}
              </div>
            ))}
            {tasks.length === 0 && !isLoading && (
              <div className="text-center text-brain-muted text-xs py-4">No tasks yet. Decompose a goal to create tasks.</div>
            )}
          </>
        )}

        {/* Goals Tab */}
        {activeTab === "goals" && (
          <>
            <div className="flex gap-2">
              <input
                type="text"
                value={goalInput}
                onChange={(e) => setGoalInput(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && handleDecompose()}
                placeholder="Enter a goal to decompose..."
                className="flex-1 bg-brain-bg/50 border border-brain-border/30 rounded-lg px-3 py-2 text-xs text-brain-text placeholder-brain-muted/50 focus:outline-none focus:border-orange-500/50"
              />
              <button
                onClick={handleDecompose}
                disabled={isDecomposing || !goalInput.trim()}
                className="px-3 py-2 rounded-lg bg-brain-accent/20 hover:bg-brain-accent/30 text-brain-accent border border-brain-accent/30 text-xs font-mono disabled:opacity-50 transition-colors"
              >
                {isDecomposing ? "..." : "Decompose"}
              </button>
            </div>

            {isDecomposing && (
              <div className="text-center text-brain-muted text-xs font-mono py-4 animate-pulse">
                Decomposing goal into tasks...
              </div>
            )}

            {decomposition && !isDecomposing && (
              <div className="space-y-2">
                <div className="bg-orange-500/10 border border-orange-500/30 rounded-lg px-3 py-2">
                  <div className="text-[10px] text-orange-400 uppercase tracking-wider mb-0.5 font-semibold">Goal</div>
                  <div className="text-xs text-brain-text">{decomposition.goal}</div>
                </div>
                <div className="text-[10px] text-brain-muted uppercase tracking-wider font-semibold">
                  Decomposed Tasks ({decomposition.tasks.length})
                </div>
                {decomposition.tasks.map((task, i) => (
                  <div
                    key={task.id || i}
                    className="flex items-start gap-2 bg-brain-bg/50 border border-brain-border/30 rounded-lg px-3 py-2"
                  >
                    <span className="text-[10px] font-mono text-orange-400/60 mt-0.5">{i + 1}.</span>
                    <div className="flex-1 min-w-0">
                      <div className="text-xs text-brain-text">{task.description}</div>
                      <div className="flex items-center gap-2 mt-0.5 text-[10px] font-mono text-brain-muted/60">
                        <span className={`px-1.5 py-0.5 rounded ${
                          STATUS_STYLES[task.status] || STATUS_STYLES.pending
                        }`}>{task.status}</span>
                        {task.assigned_agent && <span>Agent: {task.assigned_agent}</span>}
                        <span>P{task.priority}</span>
                      </div>
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
        className="mt-3 w-full py-2 rounded-lg bg-orange-500/10 text-orange-400 text-xs font-mono hover:bg-orange-500/20 transition-colors border border-orange-500/20 disabled:opacity-50"
      >
        {isLoading ? "Loading..." : "Refresh Swarm"}
      </button>
    </div>
  );
}
