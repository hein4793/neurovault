import { useEffect, useRef } from "react";
import { useGraphStore } from "@/stores/graphStore";
import { getBrainStats } from "@/lib/tauri";

/**
 * Shared stats hook. Only starts polling once loadPhase reaches "ready".
 */
export function useBrainStats() {
  const stats = useGraphStore((s) => s.stats);
  const loadPhase = useGraphStore((s) => s.loadPhase);
  const setStats = useGraphStore((s) => s.setStats);
  const intervalRef = useRef<ReturnType<typeof setInterval>>(undefined);

  useEffect(() => {
    // Don't poll until backend is ready
    if (loadPhase !== "ready") return;

    const refresh = async () => {
      try {
        const freshStats = await getBrainStats();
        setStats(freshStats);
      } catch (err) {
        // Silent — stats are non-critical
      }
    };

    // Refresh now and every 60 seconds
    refresh();
    intervalRef.current = setInterval(refresh, 60_000);

    return () => {
      if (intervalRef.current) clearInterval(intervalRef.current);
    };
  }, [loadPhase, setStats]);

  return { stats };
}
