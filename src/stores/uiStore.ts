import { create } from "zustand";

export type Panel =
  | "search" | "detail" | "ingest" | "stats" | "research"
  | "settings" | "ask" | "quality" | "learning" | "insights" | "backup"
  | "autonomy" | "context" | "activity"
  | "digital-twin" | "swarm" | "world-model" | "self-improve" | "consciousness"
  | null;

interface UiState {
  activePanel: Panel;
  sidebarCollapsed: boolean;
  showMinimap: boolean;
  showShortcutHelp: boolean;
  viewMode: "default" | "heatmap" | "cluster";
  heatmapMetric: "quality" | "decay" | "access" | "connections";
  theme: "dark";

  setActivePanel: (panel: Panel) => void;
  toggleSidebar: () => void;
  toggleMinimap: () => void;
  toggleShortcutHelp: () => void;
  setViewMode: (mode: "default" | "heatmap" | "cluster") => void;
  setHeatmapMetric: (metric: "quality" | "decay" | "access" | "connections") => void;
}

export const useUiStore = create<UiState>((set) => ({
  activePanel: null,
  sidebarCollapsed: false,
  showMinimap: true,
  showShortcutHelp: false,
  viewMode: "default",
  heatmapMetric: "quality",
  theme: "dark",

  setActivePanel: (panel) =>
    set((s) => ({ activePanel: s.activePanel === panel ? null : panel })),
  toggleSidebar: () => set((s) => ({ sidebarCollapsed: !s.sidebarCollapsed })),
  toggleMinimap: () => set((s) => ({ showMinimap: !s.showMinimap })),
  toggleShortcutHelp: () => set((s) => ({ showShortcutHelp: !s.showShortcutHelp })),
  setViewMode: (mode) => set({ viewMode: mode }),
  setHeatmapMetric: (metric) => set({ heatmapMetric: metric }),
}));
