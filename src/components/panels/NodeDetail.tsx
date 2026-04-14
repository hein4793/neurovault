import { useState, useEffect } from "react";
import { useGraphStore } from "@/stores/graphStore";
import {
  getDomainColor,
  deleteNode,
  updateNode,
  getEdgesForNode,
  createEdge,
  deleteEdge,
  GraphEdge,
  GraphNode,
  DOMAINS,
} from "@/lib/tauri";
import { formatDate } from "@/lib/utils";

export function NodeDetail() {
  const { selectedNode, selectNode, removeNode, updateNodeInStore, nodes, addEdge, removeEdge } =
    useGraphStore();
  const [isEditing, setIsEditing] = useState(false);
  const [editTitle, setEditTitle] = useState("");
  const [editContent, setEditContent] = useState("");
  const [editDomain, setEditDomain] = useState("");
  const [editTopic, setEditTopic] = useState("");
  const [editTags, setEditTags] = useState("");
  const [connectedEdges, setConnectedEdges] = useState<GraphEdge[]>([]);
  const [showConnectPicker, setShowConnectPicker] = useState(false);
  const [connectSearch, setConnectSearch] = useState("");
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (selectedNode) {
      loadEdges();
      setIsEditing(false);
    }
  }, [selectedNode?.id]);

  // Listen for Ctrl+E edit event from keyboard shortcuts
  useEffect(() => {
    const handler = () => {
      if (selectedNode && !isEditing) startEditing();
    };
    window.addEventListener("brain:edit-node", handler);
    return () => window.removeEventListener("brain:edit-node", handler);
  }, [selectedNode, isEditing]);

  const loadEdges = async () => {
    if (!selectedNode) return;
    try {
      const edges = await getEdgesForNode(selectedNode.id);
      setConnectedEdges(edges);
    } catch {
      setConnectedEdges([]);
    }
  };

  if (!selectedNode) return null;

  const domainColor = getDomainColor(selectedNode.domain);

  const startEditing = () => {
    setEditTitle(selectedNode.title);
    setEditContent(selectedNode.content);
    setEditDomain(selectedNode.domain);
    setEditTopic(selectedNode.topic);
    setEditTags(selectedNode.tags.join(", "));
    setIsEditing(true);
  };

  const cancelEditing = () => {
    setIsEditing(false);
  };

  const handleSave = async () => {
    setSaving(true);
    try {
      const updated = await updateNode({
        id: selectedNode.id,
        title: editTitle,
        content: editContent,
        domain: editDomain,
        topic: editTopic,
        tags: editTags
          .split(",")
          .map((t) => t.trim())
          .filter(Boolean),
      });
      updateNodeInStore(updated);
      setIsEditing(false);
    } catch (err) {
      console.error("Failed to update node:", err);
    } finally {
      setSaving(false);
    }
  };

  const handleDelete = async () => {
    try {
      await deleteNode(selectedNode.id);
      removeNode(selectedNode.id);
      selectNode(null);
    } catch (err) {
      console.error("Failed to delete node:", err);
    }
  };

  const handleDeleteEdge = async (edgeId: string) => {
    try {
      await deleteEdge(edgeId);
      removeEdge(edgeId);
      setConnectedEdges((prev) => prev.filter((e) => e.id !== edgeId));
    } catch (err) {
      console.error("Failed to delete edge:", err);
    }
  };

  const handleConnect = async (targetNode: GraphNode) => {
    try {
      const edge = await createEdge({
        sourceId: selectedNode.id,
        targetId: targetNode.id,
        relationType: "user_linked",
        evidence: `Manually linked by user`,
      });
      addEdge(edge);
      setConnectedEdges((prev) => [...prev, edge]);
      setShowConnectPicker(false);
      setConnectSearch("");
    } catch (err) {
      console.error("Failed to create edge:", err);
    }
  };

  // Get connected node info
  const getConnectedNode = (edge: GraphEdge): GraphNode | undefined => {
    const otherId = edge.source === selectedNode.id ? edge.target : edge.source;
    return nodes.find((n) => n.id === otherId);
  };

  // Filter nodes for connection picker
  const filteredNodes = nodes.filter(
    (n) =>
      n.id !== selectedNode.id &&
      !connectedEdges.some(
        (e) =>
          (e.source === n.id && e.target === selectedNode.id) ||
          (e.target === n.id && e.source === selectedNode.id),
      ) &&
      (connectSearch === "" ||
        n.title.toLowerCase().includes(connectSearch.toLowerCase()) ||
        n.topic.toLowerCase().includes(connectSearch.toLowerCase())),
  );

  return (
    <div className="p-4 flex flex-col h-full">
      {/* Header */}
      <div className="flex items-start justify-between mb-4">
        <div className="flex-1">
          <div className="flex items-center gap-2 mb-1">
            <div
              className="w-3 h-3 rounded-full"
              style={{ backgroundColor: domainColor, boxShadow: `0 0 8px ${domainColor}` }}
            />
            {isEditing ? (
              <select
                value={editDomain}
                onChange={(e) => setEditDomain(e.target.value)}
                className="text-xs font-mono uppercase bg-brain-bg/50 border border-brain-border/50 rounded px-2 py-0.5 text-brain-muted outline-none"
              >
                {DOMAINS.map((d) => (
                  <option key={d} value={d}>
                    {d}
                  </option>
                ))}
              </select>
            ) : (
              <span className="text-xs font-mono uppercase tracking-wider text-brain-muted">
                {selectedNode.domain}
              </span>
            )}
          </div>
          {isEditing ? (
            <input
              value={editTitle}
              onChange={(e) => setEditTitle(e.target.value)}
              className="w-full text-lg font-semibold text-brain-text bg-brain-bg/50 border border-brain-border/50 rounded-lg px-2 py-1 outline-none focus:border-brain-accent/50"
            />
          ) : (
            <h2 className="text-lg font-semibold text-brain-text leading-tight">
              {selectedNode.title}
            </h2>
          )}
        </div>
        <div className="flex items-center gap-1">
          {!isEditing && (
            <button
              onClick={startEditing}
              className="text-brain-muted hover:text-brain-accent p-1 transition-colors"
              title="Edit (Ctrl+E)"
            >
              <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z"
                />
              </svg>
            </button>
          )}
          <button
            onClick={() => selectNode(null)}
            className="text-brain-muted hover:text-brain-text p-1"
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M6 18L18 6M6 6l12 12"
              />
            </svg>
          </button>
        </div>
      </div>

      {/* Meta info */}
      <div className="flex flex-wrap gap-2 mb-3">
        <span className="text-xs px-2 py-1 rounded-full bg-brain-panel border border-brain-border/50 text-brain-muted font-mono">
          {selectedNode.node_type}
        </span>
        <span className="text-xs px-2 py-1 rounded-full bg-brain-panel border border-brain-border/50 text-brain-muted font-mono">
          {selectedNode.source_type}
        </span>
        {isEditing ? (
          <input
            value={editTopic}
            onChange={(e) => setEditTopic(e.target.value)}
            placeholder="Topic..."
            className="text-xs px-2 py-1 rounded-full bg-brain-bg/50 border border-brain-border/50 text-brain-muted font-mono outline-none w-32"
          />
        ) : (
          selectedNode.topic && (
            <span className="text-xs px-2 py-1 rounded-full bg-brain-panel border border-brain-border/50 text-brain-accent/70 font-mono">
              {selectedNode.topic}
            </span>
          )
        )}
      </div>

      {/* Tags */}
      {isEditing ? (
        <div className="mb-3">
          <input
            value={editTags}
            onChange={(e) => setEditTags(e.target.value)}
            placeholder="Tags (comma separated)..."
            className="w-full text-xs px-3 py-2 rounded-lg bg-brain-bg/50 border border-brain-border/50 text-brain-muted font-mono outline-none focus:border-brain-accent/50"
          />
        </div>
      ) : (
        selectedNode.tags.length > 0 && (
          <div className="flex flex-wrap gap-1 mb-3">
            {selectedNode.tags.map((tag) => (
              <span
                key={tag}
                className="text-xs px-2 py-0.5 rounded-full border border-brain-border/50 font-mono"
                style={{ color: domainColor, borderColor: domainColor + "30" }}
              >
                {tag}
              </span>
            ))}
          </div>
        )
      )}

      {/* Content */}
      <div className="flex-1 overflow-y-auto mb-3 min-h-0">
        {isEditing ? (
          <textarea
            value={editContent}
            onChange={(e) => setEditContent(e.target.value)}
            className="w-full h-full min-h-[200px] text-sm text-brain-text/80 leading-relaxed whitespace-pre-wrap font-mono bg-brain-bg/50 border border-brain-border/50 rounded-lg p-3 outline-none focus:border-brain-accent/50 resize-none"
          />
        ) : (
          <div className="text-sm text-brain-text/80 leading-relaxed whitespace-pre-wrap font-mono">
            {selectedNode.content}
          </div>
        )}
      </div>

      {/* Edit buttons */}
      {isEditing && (
        <div className="flex gap-2 mb-3">
          <button
            onClick={handleSave}
            disabled={saving}
            className="flex-1 py-2 rounded-lg bg-brain-accent/20 text-brain-accent text-sm font-mono hover:bg-brain-accent/30 transition-colors disabled:opacity-50 border border-brain-accent/20"
          >
            {saving ? "Saving..." : "Save Changes"}
          </button>
          <button
            onClick={cancelEditing}
            className="px-4 py-2 rounded-lg bg-brain-panel text-brain-muted text-sm font-mono hover:text-brain-text transition-colors border border-brain-border/50"
          >
            Cancel
          </button>
        </div>
      )}

      {/* Connected Nodes (Synapses) */}
      {!isEditing && (
        <div className="mb-3">
          <div className="flex items-center justify-between mb-2">
            <h3 className="text-xs font-semibold text-brain-muted uppercase tracking-wider">
              Synapses ({connectedEdges.length})
            </h3>
            <button
              onClick={() => setShowConnectPicker(!showConnectPicker)}
              className="text-[10px] px-2 py-0.5 rounded text-brain-accent hover:bg-brain-accent/10 transition-colors font-mono"
            >
              + Connect
            </button>
          </div>

          {showConnectPicker && (
            <div className="mb-2 border border-brain-border/50 rounded-lg bg-brain-bg/80 p-2">
              <input
                value={connectSearch}
                onChange={(e) => setConnectSearch(e.target.value)}
                placeholder="Search nodes to connect..."
                className="w-full text-xs px-2 py-1.5 rounded bg-brain-panel border border-brain-border/50 text-brain-text font-mono outline-none focus:border-brain-accent/50 mb-2"
                autoFocus
              />
              <div className="max-h-32 overflow-y-auto space-y-0.5">
                {filteredNodes.slice(0, 20).map((n) => (
                  <button
                    key={n.id}
                    onClick={() => handleConnect(n)}
                    className="w-full text-left text-xs font-mono px-2 py-1 rounded hover:bg-brain-accent/10 text-brain-muted hover:text-brain-text transition-colors flex items-center gap-2"
                  >
                    <div
                      className="w-2 h-2 rounded-full flex-shrink-0"
                      style={{ backgroundColor: getDomainColor(n.domain) }}
                    />
                    <span className="truncate">{n.title}</span>
                  </button>
                ))}
                {filteredNodes.length === 0 && (
                  <div className="text-xs text-brain-muted/50 text-center py-2 font-mono">
                    No nodes found
                  </div>
                )}
              </div>
            </div>
          )}

          <div className="space-y-1 max-h-40 overflow-y-auto">
            {connectedEdges.map((edge) => {
              const other = getConnectedNode(edge);
              if (!other) return null;
              return (
                <div
                  key={edge.id}
                  className="flex items-center gap-2 text-xs font-mono px-2 py-1.5 rounded bg-brain-bg/30 group"
                >
                  <div
                    className="w-2 h-2 rounded-full flex-shrink-0"
                    style={{ backgroundColor: getDomainColor(other.domain) }}
                  />
                  <button
                    onClick={() => selectNode(other)}
                    className="truncate text-brain-muted hover:text-brain-text transition-colors text-left flex-1"
                  >
                    {other.title}
                  </button>
                  <span className="text-brain-muted/40 flex-shrink-0">{edge.relation_type}</span>
                  <button
                    onClick={() => handleDeleteEdge(edge.id)}
                    className="text-red-400/0 group-hover:text-red-400/60 hover:!text-red-400 transition-colors flex-shrink-0"
                    title="Remove connection"
                  >
                    <svg className="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path
                        strokeLinecap="round"
                        strokeLinejoin="round"
                        strokeWidth={2}
                        d="M6 18L18 6M6 6l12 12"
                      />
                    </svg>
                  </button>
                </div>
              );
            })}
          </div>
        </div>
      )}

      {/* Footer */}
      <div className="border-t border-brain-border/30 pt-3 flex items-center justify-between">
        <div className="text-xs text-brain-muted font-mono">
          <div>Created: {formatDate(selectedNode.created_at)}</div>
          <div>Views: {selectedNode.access_count}</div>
        </div>
        {!isEditing && (
          <button
            onClick={handleDelete}
            className="text-xs px-3 py-1.5 rounded-lg bg-red-500/10 text-red-400 hover:bg-red-500/20 transition-colors border border-red-500/20"
          >
            Delete
          </button>
        )}
      </div>
    </div>
  );
}
