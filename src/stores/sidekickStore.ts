import { create } from "zustand";

// ===== Types =====

export interface ActivityItem {
  id: number;
  timestamp: number;
  type: "node" | "edge" | "ingestion" | "research" | "autonomy" | "learning" | "export" | "system";
  severity: "info" | "success" | "warning" | "error";
  message: string;
  detail?: string;
  nodeId?: string;
}

export type VisualEffectType = "particle_burst" | "pulse_wave" | "color_shift" | "ring_flash" | "lightning";

export interface VisualEffect {
  type: VisualEffectType;
  color?: string;
  intensity?: number;
  position?: [number, number, number];
  domain?: string;
}

export type BrainState = "idle" | "thinking" | "learning" | "linking" | "exporting";

export interface BrainVitals {
  activityLevel: number;    // 0-1, drives breathing/impulse intensity
  state: BrainState;
  currentIq: number;
  previousIq: number;
  nodeCount: number;
  edgeCount: number;
  activeTaskCount: number;
}

// ===== Store =====

interface SidekickState {
  // Activity feed (ring buffer, max 100)
  activityFeed: ActivityItem[];

  // Brain vitals (drives 3D effects)
  vitals: BrainVitals;

  // Visual effect queue (consumed by 3D layer each frame)
  pendingEffects: VisualEffect[];

  // UI state
  feedExpanded: boolean;
  unreadCount: number;

  // Actions
  pushActivity: (item: Omit<ActivityItem, "id" | "timestamp">) => void;
  queueEffect: (effect: VisualEffect) => void;
  consumeEffects: () => VisualEffect[];
  updateVitals: (partial: Partial<BrainVitals>) => void;
  setActivityLevel: (level: number) => void;
  setBrainState: (state: BrainState) => void;
  toggleFeedExpanded: () => void;
  markRead: () => void;
}

let _nextActivityId = 1;

export const useSidekickStore = create<SidekickState>((set, get) => ({
  activityFeed: [],
  vitals: {
    activityLevel: 0,
    state: "idle",
    currentIq: 0,
    previousIq: 0,
    nodeCount: 0,
    edgeCount: 0,
    activeTaskCount: 0,
  },
  pendingEffects: [],
  feedExpanded: false,
  unreadCount: 0,

  pushActivity: (item) =>
    set((s) => {
      const newItem: ActivityItem = {
        ...item,
        id: _nextActivityId++,
        timestamp: Date.now(),
      };
      // Ring buffer: keep last 100
      const feed = [newItem, ...s.activityFeed].slice(0, 100);
      return {
        activityFeed: feed,
        unreadCount: s.feedExpanded ? 0 : s.unreadCount + 1,
      };
    }),

  queueEffect: (effect) =>
    set((s) => ({
      // Cap at 10 pending effects to prevent burst overload
      pendingEffects: [...s.pendingEffects, effect].slice(-10),
    })),

  consumeEffects: () => {
    const effects = get().pendingEffects;
    if (effects.length === 0) return [];
    // Consume up to 3 per frame
    const batch = effects.slice(0, 3);
    set({ pendingEffects: effects.slice(3) });
    return batch;
  },

  updateVitals: (partial) =>
    set((s) => ({
      vitals: { ...s.vitals, ...partial },
    })),

  setActivityLevel: (level) =>
    set((s) => ({
      vitals: { ...s.vitals, activityLevel: Math.max(0, Math.min(1, level)) },
    })),

  setBrainState: (state) =>
    set((s) => ({
      vitals: { ...s.vitals, state },
    })),

  toggleFeedExpanded: () =>
    set((s) => ({
      feedExpanded: !s.feedExpanded,
      unreadCount: !s.feedExpanded ? 0 : s.unreadCount,
    })),

  markRead: () => set({ unreadCount: 0 }),
}));

// Activity level decay timer — decays toward 0 when idle
let _decayInterval: ReturnType<typeof setInterval> | null = null;

export function startActivityDecay() {
  if (_decayInterval) return;
  _decayInterval = setInterval(() => {
    const store = useSidekickStore.getState();
    if (store.vitals.activityLevel > 0.01) {
      store.setActivityLevel(store.vitals.activityLevel * 0.85);
    } else if (store.vitals.activityLevel > 0) {
      store.setActivityLevel(0);
    }
  }, 5000);
}

export function stopActivityDecay() {
  if (_decayInterval) {
    clearInterval(_decayInterval);
    _decayInterval = null;
  }
}
