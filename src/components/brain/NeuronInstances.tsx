import { useMemo, useCallback, useRef } from "react";
import { ThreeEvent } from "@react-three/fiber";
import { Instances, Instance } from "@react-three/drei";
import * as THREE from "three";
import { useGraphStore } from "@/stores/graphStore";
import { useUiStore } from "@/stores/uiStore";
import { useBrainContainment } from "@/hooks/useBrainContainment";
import { getDomainColor, GraphNode } from "@/lib/tauri";
import { BRAIN_CENTER_Y, SKILL_AGENTS, DOMAIN_COLORS_RGB } from "@/lib/constants";

interface PositionedNode {
  node: GraphNode;
  position: [number, number, number];
  color: string;
  scale: number;
}

export function NeuronInstances() {
  const nodes = useGraphStore((s) => s.nodes);
  const edges = useGraphStore((s) => s.edges);
  const selectedNode = useGraphStore((s) => s.selectedNode);
  const highlightedNodes = useGraphStore((s) => s.highlightedNodes);
  const selectNode = useGraphStore((s) => s.selectNode);
  const setHoveredNode = useGraphStore((s) => s.setHoveredNode);
  const { viewMode, heatmapMetric } = useUiStore();
  const meshReady = useBrainContainment((s) => s.ready);
  const getPosition = useBrainContainment((s) => s.getPosition);

  // Build skill agent nodes
  const skillNodes: GraphNode[] = useMemo(() =>
    SKILL_AGENTS.map((name, i) => ({
      id: `__skill_${i}__`,
      title: `${name} Expert`,
      content: `Specialized AI agent for ${name}.`,
      summary: `${name} skill agent`,
      domain: "technology",
      topic: name.toLowerCase().replace(/\s/g, "-"),
      tags: ["skill", "agent"],
      node_type: "skill",
      source_type: "system",
      visual_size: 2.5,
      access_count: 0,
      decay_score: 1.0,
      created_at: new Date().toISOString(),
    })),
  []);

  // Position all nodes inside the brain mesh
  const positionedNodes: PositionedNode[] = useMemo(() => {
    const allNodes = [...nodes, ...skillNodes];
    return allNodes.map((node) => {
      const pos = getPosition();
      const baseScale = node.visual_size || 4;
      let color = getDomainColor(node.domain);

      // Heatmap mode color override
      if (viewMode === "heatmap") {
        color = getHeatmapColor(node, heatmapMetric);
      }

      return { node, position: pos, color, scale: baseScale };
    });
  }, [nodes, skillNodes, meshReady, viewMode, heatmapMetric, getPosition]);

  const handleClick = useCallback((node: GraphNode, e: ThreeEvent<MouseEvent>) => {
    e.stopPropagation();
    if (node.id.startsWith("__skill_")) return;
    selectNode(node);
  }, [selectNode]);

  const handlePointerOver = useCallback((node: GraphNode) => {
    if (!node.id.startsWith("__skill_")) {
      setHoveredNode(node);
      document.body.style.cursor = "pointer";
    }
  }, [setHoveredNode]);

  const handlePointerOut = useCallback(() => {
    setHoveredNode(null);
    document.body.style.cursor = "auto";
  }, [setHoveredNode]);

  if (positionedNodes.length === 0) return null;

  // Split into regular neurons and skill agents
  const regularNodes = positionedNodes.filter((p) => p.node.node_type !== "skill");
  const skillNodePositions = positionedNodes.filter((p) => p.node.node_type === "skill");

  return (
    <>
      {/* Regular neurons - spheres */}
      <Instances limit={700} range={regularNodes.length}>
        <sphereGeometry args={[1, 8, 6]} />
        <meshPhongMaterial
          transparent
          opacity={0.85}
          depthWrite={false}
          blending={THREE.AdditiveBlending}
          shininess={80}
        />
        {regularNodes.map((pn) => {
          const isSelected = selectedNode?.id === pn.node.id;
          const isHighlighted = highlightedNodes.has(pn.node.id);
          const finalScale = pn.scale * (isSelected ? 2.0 : isHighlighted ? 1.5 : 1.0);

          return (
            <Instance
              key={pn.node.id}
              position={pn.position}
              scale={finalScale}
              color={pn.color}
              onClick={(e: ThreeEvent<MouseEvent>) => handleClick(pn.node, e)}
              onPointerOver={() => handlePointerOver(pn.node)}
              onPointerOut={handlePointerOut}
            />
          );
        })}
      </Instances>

      {/* Skill agents - octahedrons */}
      <Instances limit={25} range={skillNodePositions.length}>
        <octahedronGeometry args={[1, 0]} />
        <meshPhongMaterial
          transparent
          opacity={0.7}
          depthWrite={false}
          blending={THREE.AdditiveBlending}
          shininess={120}
        />
        {skillNodePositions.map((pn) => (
          <Instance
            key={pn.node.id}
            position={pn.position}
            scale={pn.scale}
            color="#22d3ee"
          />
        ))}
      </Instances>

      {/* Selection ring for selected node */}
      {selectedNode && (() => {
        const selected = positionedNodes.find((p) => p.node.id === selectedNode.id);
        if (!selected) return null;
        return (
          <mesh position={selected.position}>
            <torusGeometry args={[selected.scale * 2.5, 0.3, 8, 32]} />
            <meshBasicMaterial
              color="#ffffff"
              transparent
              opacity={0.6}
              depthWrite={false}
              blending={THREE.AdditiveBlending}
            />
          </mesh>
        );
      })()}
    </>
  );
}

function getHeatmapColor(node: GraphNode, metric: string): string {
  let value = 0;
  switch (metric) {
    case "quality":
      // visual_size correlates with quality (set by backend)
      value = Math.min(1, (node.visual_size || 4) / 8);
      break;
    case "decay":
      value = node.decay_score;
      break;
    case "access":
      value = Math.min(1, node.access_count / 50);
      break;
    case "connections":
      value = Math.min(1, (node.visual_size || 4) / 10);
      break;
  }

  // Gradient: red (0) → yellow (0.5) → green (1.0)
  const r = value < 0.5 ? 1.0 : 1.0 - (value - 0.5) * 2;
  const g = value < 0.5 ? value * 2 : 1.0;
  const b = 0.1;
  return `rgb(${Math.floor(r * 255)}, ${Math.floor(g * 255)}, ${Math.floor(b * 255)})`;
}
