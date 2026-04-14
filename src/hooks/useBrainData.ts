import { useEffect } from "react";
import { useGraphStore } from "@/stores/graphStore";
import { useSidekickStore } from "@/stores/sidekickStore";
import { getBrainStats, getNodeCount, getEdgeCount, getAllNodes, analyzeTrends } from "@/lib/tauri";

/**
 * Brain data loading hook.
 * Waits for backend DB to be ready (can take 90s on cold start),
 * then signals cloud to load and fetches the top 600 nodes for
 * the interactive 3D visualization layer.
 */
export function useBrainData() {
  useEffect(() => {
    let cancelled = false;
    const store = () => useGraphStore.getState();
    const vitals = () => useSidekickStore.getState();
    store().setLoadPhase("init");

    async function waitForBackend() {
      if (cancelled) return;
      console.log("[BrainData] Checking if backend is ready...");

      try {
        // getNodeCount is a fast query — if it works, DB is ready
        const count = await getNodeCount();
        if (cancelled) return;
        console.log(`[BrainData] Backend ready! ${count} nodes in DB`);

        // DB is ready — immediately show node count in status bar
        vitals().updateVitals({ nodeCount: count });

        // Also fetch a fast edge count so synapses don't sit at 0 while
        // the heavy get_brain_stats runs in the background.
        getEdgeCount()
          .then((edges) => {
            if (cancelled) return;
            console.log(`[BrainData] Edge count: ${edges}`);
            vitals().updateVitals({ edgeCount: edges });
          })
          .catch(() => console.warn("[BrainData] Edge count failed (non-critical)"));

        // Move to cloud phase so NeuronCloud starts loading
        store().setLoadPhase("cloud");

        // Mark as ready immediately — don't wait for slow stats/IQ queries
        if (!cancelled) store().setLoadPhase("ready");

        // Fire nodes + stats + IQ loads in background (don't block UI)
        loadNodesBackground(cancelled);
        loadStatsBackground(cancelled);
        loadIqBackground(cancelled);
      } catch (err) {
        // DB not ready yet — retry in 5 seconds
        console.log("[BrainData] Backend not ready, retrying in 5s...");
        if (!cancelled) setTimeout(waitForBackend, 5000);
      }
    }

    async function loadNodesBackground(wasCancelled: boolean) {
      try {
        const nodes = await getAllNodes();
        if (!wasCancelled && nodes.length > 0) {
          store().setNodes(nodes);
          console.log(`[BrainData] Loaded ${nodes.length} top nodes for 3D visualization`);
        }
      } catch {
        console.warn("[BrainData] Node fetch failed (non-critical)");
      }
    }

    async function loadStatsBackground(wasCancelled: boolean) {
      try {
        const stats = await getBrainStats();
        if (!wasCancelled) {
          store().setStats(stats);
          vitals().updateVitals({
            nodeCount: stats.total_nodes,
            edgeCount: stats.total_edges,
          });
          console.log(`[BrainData] Stats: ${stats.total_nodes} nodes, ${stats.total_edges} edges`);
        }
      } catch {
        console.warn("[BrainData] Stats failed (non-critical)");
      }
    }

    async function loadIqBackground(wasCancelled: boolean) {
      try {
        const trends = await analyzeTrends();
        if (!wasCancelled && trends) {
          const iq = Math.round(trends.brain_iq);
          vitals().updateVitals({ currentIq: iq });
          console.log(`[BrainData] IQ: ${iq}/200`);
        }
      } catch {
        console.warn("[BrainData] IQ fetch failed (non-critical)");
      }
    }

    // Start checking after 3 seconds (give Rust time to start)
    const timer = setTimeout(waitForBackend, 3000);

    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, []);

  const loadPhase = useGraphStore((s) => s.loadPhase);
  return { loadPhase };
}
