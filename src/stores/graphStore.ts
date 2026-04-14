import { create } from "zustand";
import { GraphNode, GraphEdge, BrainStats, NodeCloud } from "@/lib/tauri";

export type LoadPhase = "init" | "mesh" | "nodes" | "cloud" | "ready";

interface GraphState {
  // Core data
  nodes: GraphNode[];
  edges: GraphEdge[];

  // Selection
  selectedNode: GraphNode | null;
  hoveredNode: GraphNode | null;
  highlightedNodes: Set<string>;

  // Search
  searchQuery: string;
  searchResults: GraphNode[];

  // Loading
  isLoading: boolean;
  loadPhase: LoadPhase;

  // Stats (shared single source of truth)
  stats: BrainStats | null;

  // Cloud data (stored once, not in component state)
  cloudData: NodeCloud | null;

  // Node actions
  setNodes: (nodes: GraphNode[]) => void;
  addNode: (node: GraphNode) => void;
  addNodes: (nodes: GraphNode[]) => void;
  updateNodeInStore: (node: GraphNode) => void;
  removeNode: (id: string) => void;

  // Edge actions
  setEdges: (edges: GraphEdge[]) => void;
  addEdge: (edge: GraphEdge) => void;
  removeEdge: (id: string) => void;

  // Selection actions
  selectNode: (node: GraphNode | null) => void;
  setHoveredNode: (node: GraphNode | null) => void;
  setHighlightedNodes: (ids: Set<string>) => void;

  // Search actions
  setSearchQuery: (query: string) => void;
  setSearchResults: (results: GraphNode[]) => void;

  // Loading actions
  setLoading: (loading: boolean) => void;
  setLoadPhase: (phase: LoadPhase) => void;

  // Stats actions
  setStats: (stats: BrainStats) => void;
  incrementNodeCount: (n: number) => void;
  incrementEdgeCount: (n: number) => void;

  // Cloud actions
  setCloudData: (data: NodeCloud) => void;
}

export const useGraphStore = create<GraphState>((set, get) => ({
  nodes: [],
  edges: [],
  selectedNode: null,
  hoveredNode: null,
  highlightedNodes: new Set(),
  searchQuery: "",
  searchResults: [],
  isLoading: false,
  loadPhase: "init",
  stats: null,
  cloudData: null,

  // Node actions
  setNodes: (nodes) => set({ nodes }),
  addNode: (node) => set((s) => ({ nodes: [...s.nodes, node] })),
  addNodes: (nodes) => set((s) => ({ nodes: [...s.nodes, ...nodes] })),
  updateNodeInStore: (node) =>
    set((s) => ({
      nodes: s.nodes.map((n) => (n.id === node.id ? node : n)),
      selectedNode: s.selectedNode?.id === node.id ? node : s.selectedNode,
    })),
  removeNode: (id) =>
    set((s) => ({
      nodes: s.nodes.filter((n) => n.id !== id),
      edges: s.edges.filter((e) => e.source !== id && e.target !== id),
      selectedNode: s.selectedNode?.id === id ? null : s.selectedNode,
      hoveredNode: s.hoveredNode?.id === id ? null : s.hoveredNode,
    })),

  // Edge actions
  setEdges: (edges) => set({ edges }),
  addEdge: (edge) => set((s) => ({ edges: [...s.edges, edge] })),
  removeEdge: (id) =>
    set((s) => ({ edges: s.edges.filter((e) => e.id !== id) })),

  // Selection
  selectNode: (node) => set({ selectedNode: node }),
  setHoveredNode: (node) => set({ hoveredNode: node }),
  setHighlightedNodes: (ids) => set({ highlightedNodes: ids }),

  // Search
  setSearchQuery: (query) => set({ searchQuery: query }),
  setSearchResults: (results) => set({ searchResults: results }),

  // Loading
  setLoading: (loading) => set({ isLoading: loading }),
  setLoadPhase: (phase) => set({ loadPhase: phase }),

  // Stats — single source of truth
  setStats: (stats) => set({ stats }),
  incrementNodeCount: (n) =>
    set((s) => {
      if (!s.stats) return {};
      return { stats: { ...s.stats, total_nodes: s.stats.total_nodes + n } };
    }),
  incrementEdgeCount: (n) =>
    set((s) => {
      if (!s.stats) return {};
      return { stats: { ...s.stats, total_edges: s.stats.total_edges + n } };
    }),

  // Cloud
  setCloudData: (data) => set({ cloudData: data }),
}));
