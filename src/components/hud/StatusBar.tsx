import { useState } from "react";
import { useGraphStore } from "@/stores/graphStore";
import { useSidekickStore, type BrainState } from "@/stores/sidekickStore";
import {
  maximizeIq,
  autoLinkNodes,
  enhanceQuality,
  boostIq,
  qualitySweep,
  deepLearn,
  calculateQuality,
  calculateDecay,
} from "@/lib/tauri";

const STATE_LABELS: Record<BrainState, string> = {
  idle: "Idle",
  thinking: "Thinking...",
  learning: "Learning...",
  linking: "Linking...",
  exporting: "Exporting...",
};

const STATE_COLORS: Record<BrainState, string> = {
  idle: "bg-green-500",
  thinking: "bg-amber-500",
  learning: "bg-purple-500",
  linking: "bg-cyan-500",
  exporting: "bg-yellow-500",
};

export function StatusBar() {
  const stats = useGraphStore((s) => s.stats);
  const loadPhase = useGraphStore((s) => s.loadPhase);
  const vitals = useSidekickStore((s) => s.vitals);
  const [optimizing, setOptimizing] = useState(false);
  const [optimizeStatus, setOptimizeStatus] = useState("");

  const cloudData = useGraphStore((s) => s.cloudData);
  const neurons = stats?.total_nodes ?? cloudData?.count ?? vitals.nodeCount;
  const synapses = stats?.total_edges ?? vitals.edgeCount;
  const brainState = vitals.state;
  const iq = vitals.currentIq;

  const handleMaximizeBrain = async () => {
    if (optimizing) return;
    setOptimizing(true);
    const { setBrainState, pushActivity } = useSidekickStore.getState();

    try {
      setBrainState("linking");
      setOptimizeStatus("Linking neurons...");
      pushActivity({ type: "autonomy", severity: "info", message: "Maximize Brain: auto-linking..." });
      await autoLinkNodes();

      setBrainState("thinking");
      setOptimizeStatus("Calculating quality...");
      pushActivity({ type: "autonomy", severity: "info", message: "Maximize Brain: quality scan..." });
      await calculateQuality();

      setOptimizeStatus("Calculating decay...");
      await calculateDecay();

      setBrainState("learning");
      setOptimizeStatus("Enhancing quality...");
      pushActivity({ type: "autonomy", severity: "info", message: "Maximize Brain: enhancing..." });
      await enhanceQuality();

      setOptimizeStatus("Quality sweep...");
      pushActivity({ type: "autonomy", severity: "info", message: "Maximize Brain: quality sweep..." });
      await qualitySweep();

      setOptimizeStatus("Boosting IQ...");
      pushActivity({ type: "autonomy", severity: "info", message: "Maximize Brain: IQ boost..." });
      await boostIq();

      setOptimizeStatus("Deep learning...");
      pushActivity({ type: "autonomy", severity: "info", message: "Maximize Brain: deep learning..." });
      await deepLearn();

      setOptimizeStatus("Final optimization...");
      pushActivity({ type: "autonomy", severity: "info", message: "Maximize Brain: final optimization..." });
      const result = await maximizeIq();

      pushActivity({ type: "autonomy", severity: "success", message: `Brain maximized! ${result}` });
      setOptimizeStatus("Done!");
      setTimeout(() => setOptimizeStatus(""), 5000);
    } catch (err) {
      console.error("Maximize failed:", err);
      pushActivity({ type: "autonomy", severity: "error", message: `Maximize failed: ${err}` });
      setOptimizeStatus("Failed");
      setTimeout(() => setOptimizeStatus(""), 5000);
    } finally {
      setOptimizing(false);
      setBrainState("idle");
    }
  };

  if (loadPhase === "init" || loadPhase === "mesh") {
    return (
      <div className="glass-panel px-4 py-2 flex items-center gap-6 text-xs font-mono text-brain-muted">
        <div className="flex items-center gap-2">
          <div className="w-2 h-2 rounded-full bg-amber-500 animate-pulse" />
          <span>Connecting to brain...</span>
        </div>
        <span className="text-brain-accent/50">NeuroVault v1.0</span>
      </div>
    );
  }

  return (
    <div className="glass-panel px-4 py-2 flex items-center gap-4 text-xs font-mono text-brain-muted">
      {/* Brain state */}
      <div className="flex items-center gap-2">
        <div className={`w-2 h-2 rounded-full ${STATE_COLORS[brainState]} ${brainState !== "idle" ? "animate-pulse" : ""}`} />
        <span className="text-brain-text/70">{STATE_LABELS[brainState]}</span>
      </div>

      {/* Neuron count */}
      <div className="flex items-center gap-2">
        <div className="w-2 h-2 rounded-full bg-brain-accent" />
        <span>{neurons.toLocaleString()} neurons</span>
      </div>

      {/* Synapse count */}
      <div className="flex items-center gap-2">
        <div className="w-2 h-2 rounded-full bg-brain-research" />
        <span>{synapses.toLocaleString()} synapses</span>
      </div>

      {/* Brain IQ */}
      {iq > 0 && (
        <div className="flex items-center gap-1">
          <span className="text-brain-accent">IQ:</span>
          <span className="text-brain-accent font-bold">{iq}</span>
          <span className="text-brain-muted">/200</span>
        </div>
      )}

      {/* Maximize Brain button */}
      <button
        onClick={handleMaximizeBrain}
        disabled={optimizing}
        className={`px-3 py-1 rounded text-[10px] font-bold uppercase tracking-wider transition-all
          ${optimizing
            ? "bg-amber-500/20 text-amber-400 cursor-wait animate-pulse"
            : "bg-brain-accent/20 text-brain-accent hover:bg-brain-accent/30 hover:text-white"
          }`}
      >
        {optimizing ? optimizeStatus || "Optimizing..." : "Maximize Brain"}
      </button>

      <span className="text-brain-accent/50">v1.0</span>
    </div>
  );
}
