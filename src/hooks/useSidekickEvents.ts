import { useEffect } from "react";
import { onBrainEvent, BrainEventPayload, getBrainStats } from "@/lib/tauri";
import { useGraphStore } from "@/stores/graphStore";
import {
  useSidekickStore,
  startActivityDecay,
  type ActivityItem,
  type VisualEffect,
  type BrainState,
} from "@/stores/sidekickStore";

/**
 * Centralized event dispatch hook.
 * Translates raw backend events into human-readable activity items,
 * queues visual effects, and updates brain vitals.
 *
 * Must be initialized once at the App level.
 *
 * Uses a module-level flag + dedup window to survive React StrictMode's
 * mount-unmount-remount cycle without leaking duplicate listeners.
 */

// Module-level dedup: track last processed event to ignore StrictMode doubles.
let _lastEventKey = "";
let _lastEventTime = 0;
const DEDUP_WINDOW_MS = 50; // events within 50ms with same key are duplicates

export function useSidekickEvents() {
  useEffect(() => {
    // Start activity decay timer (idempotent — won't start twice)
    startActivityDecay();

    // Listen to brain events — store the unlisten promise so cleanup
    // can await it even if the promise hasn't resolved yet.
    let unlistenFn: (() => void) | null = null;
    let cancelled = false;

    const listenerPromise = onBrainEvent((event) => {
      if (cancelled) return;

      // Dedup: skip if same event type + key arrived within the window.
      // This prevents StrictMode double-fire and backend double-emit.
      const eventKey = `${event.type}:${event.id || event.title || event.task || event.topic || ""}`;
      const now = Date.now();
      if (eventKey === _lastEventKey && now - _lastEventTime < DEDUP_WINDOW_MS) {
        return; // duplicate — skip
      }
      _lastEventKey = eventKey;
      _lastEventTime = now;

      processEvent(event);
    });

    listenerPromise.then((fn) => {
      if (cancelled) {
        // Effect was already cleaned up before the listener registered —
        // immediately unsubscribe to prevent a leaked listener.
        fn();
      } else {
        unlistenFn = fn;
      }
    });

    // Periodic context refresh (stats, etc.)
    const refreshInterval = setInterval(async () => {
      if (cancelled) return;
      try {
        const stats = await getBrainStats();
        useGraphStore.getState().setStats(stats);
        useSidekickStore.getState().updateVitals({
          nodeCount: stats.total_nodes,
          edgeCount: stats.total_edges,
        });
      } catch {
        // Silently fail — not critical
      }
    }, 60_000);

    return () => {
      cancelled = true;
      if (unlistenFn) unlistenFn();
      clearInterval(refreshInterval);
    };
  }, []);
}

function processEvent(event: BrainEventPayload) {
  const { pushActivity, queueEffect, setActivityLevel, setBrainState, updateVitals } =
    useSidekickStore.getState();
  const { incrementNodeCount, incrementEdgeCount } = useGraphStore.getState();
  const currentActivity = useSidekickStore.getState().vitals.activityLevel;

  let activity: Omit<ActivityItem, "id" | "timestamp"> | null = null;
  let effect: VisualEffect | null = null;
  let activityBoost = 0;
  let newState: BrainState | null = null;

  switch (event.type) {
    case "NodeCreated":
      activity = {
        type: "node",
        severity: "success",
        message: `New neuron: ${event.title || "untitled"}`,
        nodeId: event.id,
      };
      effect = { type: "particle_burst", intensity: 0.15, domain: event.source };
      activityBoost = 0.1;
      incrementNodeCount(1);
      break;

    case "NodeUpdated":
      activity = {
        type: "node",
        severity: "info",
        message: `Updated: ${event.title || "node"}`,
        nodeId: event.id,
      };
      activityBoost = 0.02;
      break;

    case "NodeDeleted":
      activity = {
        type: "node",
        severity: "warning",
        message: `Removed neuron: ${event.title || event.id}`,
      };
      incrementNodeCount(-1);
      break;

    case "EdgeCreated":
      activity = {
        type: "edge",
        severity: "info",
        message: "Synapse formed",
        detail: event.source && event.target ? `${event.source} ↔ ${event.target}` : undefined,
      };
      effect = { type: "lightning", intensity: 0.1 };
      activityBoost = 0.05;
      incrementEdgeCount(1);
      break;

    case "EdgeDeleted":
      incrementEdgeCount(-1);
      break;

    case "IngestionStarted":
      activity = {
        type: "ingestion",
        severity: "info",
        message: `Ingesting: ${event.source || "source"}...`,
      };
      effect = { type: "color_shift", color: "#f59e0b", intensity: 0.2 };
      newState = "thinking";
      activityBoost = 0.15;
      break;

    case "IngestionCompleted":
      activity = {
        type: "ingestion",
        severity: "success",
        message: `Ingested ${event.nodes_created || 0} neurons`,
        detail: event.source || undefined,
      };
      effect = { type: "particle_burst", intensity: 0.3 };
      activityBoost = 0.3;
      newState = "idle";
      break;

    case "IngestionFailed":
      activity = {
        type: "ingestion",
        severity: "error",
        message: `Ingestion failed: ${event.error || "unknown"}`,
      };
      newState = "idle";
      break;

    case "AutoLinkCompleted":
      activity = {
        type: "autonomy",
        severity: "success",
        message: `Auto-linked ${event.created || 0} new synapses`,
        detail: event.total_nodes ? `across ${event.total_nodes} neurons` : undefined,
      };
      effect = { type: "pulse_wave", color: "#38bdf8", intensity: 0.25 };
      activityBoost = 0.2;
      newState = "idle";
      if (event.created) incrementEdgeCount(event.created);
      break;

    case "ResearchStarted":
      activity = {
        type: "research",
        severity: "info",
        message: `Researching: "${event.topic || "topic"}"...`,
      };
      effect = { type: "ring_flash", color: "#8b5cf6", intensity: 0.2 };
      newState = "learning";
      activityBoost = 0.2;
      break;

    case "ResearchCompleted":
      activity = {
        type: "research",
        severity: "success",
        message: `Learned ${event.nodes_created || 0} things about "${event.topic || "topic"}"`,
      };
      effect = { type: "particle_burst", color: "#8b5cf6", intensity: 0.4 };
      activityBoost = 0.4;
      newState = "idle";
      break;

    case "AutonomyTaskStarted":
      activity = {
        type: "autonomy",
        severity: "info",
        message: `Brain task: ${formatTaskName(event.task)}`,
      };
      newState = taskToState(event.task);
      activityBoost = 0.1;
      break;

    case "AutonomyTaskCompleted":
      activity = {
        type: "autonomy",
        severity: "success",
        message: `${formatTaskName(event.task)}: ${event.result || "done"}`,
        detail: event.duration_ms ? `${(event.duration_ms / 1000).toFixed(1)}s` : undefined,
      };
      effect = { type: "pulse_wave", color: "#00cc88", intensity: 0.2 };
      activityBoost = 0.2;
      newState = "idle";
      break;

    case "AutonomyTaskFailed":
      activity = {
        type: "autonomy",
        severity: "error",
        message: `${formatTaskName(event.task)} failed`,
        detail: event.error || undefined,
      };
      newState = "idle";
      break;

    case "ActiveLearningCompleted":
      activity = {
        type: "learning",
        severity: "success",
        message: `Autonomous learning: ${event.topics_researched || 0} topics, ${event.nodes_created || 0} new neurons`,
      };
      effect = { type: "particle_burst", color: "#22d3ee", intensity: 0.5 };
      activityBoost = 0.5;
      newState = "idle";
      break;

    case "BriefingUpdated":
      activity = {
        type: "export",
        severity: "info",
        message: `Brain IQ: ${event.iq || "?"}/200`,
      };
      if (event.iq) {
        const prev = useSidekickStore.getState().vitals.currentIq;
        updateVitals({ previousIq: prev, currentIq: event.iq });
        if (event.iq > prev) {
          effect = { type: "pulse_wave", color: "#ffd700", intensity: 0.3 };
        }
      }
      break;

    default:
      // Unknown event type — log but don't display
      console.log("Brain event:", event.type, event);
      return;
  }

  // Push activity to feed
  if (activity) {
    pushActivity(activity);
  }

  // Queue visual effect
  if (effect) {
    queueEffect(effect);
  }

  // Update activity level
  if (activityBoost > 0) {
    setActivityLevel(Math.min(1, currentActivity + activityBoost));
  }

  // Update brain state
  if (newState) {
    setBrainState(newState);
  }
}

function formatTaskName(task?: string): string {
  if (!task) return "Unknown task";
  return task
    .replace(/_/g, " ")
    .replace(/\b\w/g, (c) => c.toUpperCase());
}

function taskToState(task?: string): BrainState {
  if (!task) return "thinking";
  if (task.includes("link")) return "linking";
  if (task.includes("learn") || task.includes("research")) return "learning";
  if (task.includes("export")) return "exporting";
  return "thinking";
}
