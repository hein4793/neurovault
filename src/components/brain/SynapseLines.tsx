import { useMemo } from "react";
import * as THREE from "three";
import { useGraphStore } from "@/stores/graphStore";
import { useBrainContainment } from "@/hooks/useBrainContainment";
import { DOMAIN_COLORS_RGB } from "@/lib/constants";
import { getDomainColor } from "@/lib/tauri";

export function SynapseLines() {
  const nodes = useGraphStore((s) => s.nodes);
  const edges = useGraphStore((s) => s.edges);
  const meshReady = useBrainContainment((s) => s.ready);
  const getPosition = useBrainContainment((s) => s.getPosition);

  // Build a position map for visible nodes
  const nodePositions = useMemo(() => {
    const map = new Map<string, [number, number, number]>();
    nodes.forEach((n) => {
      map.set(n.id, getPosition());
    });
    return map;
  }, [nodes, meshReady]);

  // Generate line segments
  const { positions, colors } = useMemo(() => {
    const posArr: number[] = [];
    const colArr: number[] = [];

    // Helper to add a line segment
    const addLine = (
      p1: [number, number, number],
      p2: [number, number, number],
      r: number, g: number, b: number,
      opacity: number
    ) => {
      posArr.push(p1[0], p1[1], p1[2], p2[0], p2[1], p2[2]);
      colArr.push(r * opacity, g * opacity, b * opacity, r * opacity, g * opacity, b * opacity);
    };

    // Group nodes by domain for within-domain chain links
    const domainGroups: Record<string, string[]> = {};
    nodes.forEach((n) => {
      (domainGroups[n.domain] ||= []).push(n.id);
    });

    // Within-domain chain links
    for (const [domain, ids] of Object.entries(domainGroups)) {
      const rgb = DOMAIN_COLORS_RGB[domain] || [0.22, 0.74, 0.97];
      const step = Math.max(1, Math.floor(ids.length / 80));
      for (let i = 0; i < ids.length - step; i += step) {
        const p1 = nodePositions.get(ids[i]);
        const p2 = nodePositions.get(ids[i + step]);
        if (p1 && p2) {
          addLine(p1, p2, rgb[0], rgb[1], rgb[2], 0.3);
        }
      }
    }

    // Cross-domain links (sample ~50)
    const domains = Object.keys(domainGroups);
    for (let i = 0; i < domains.length; i++) {
      for (let j = i + 1; j < domains.length; j++) {
        const g1 = domainGroups[domains[i]];
        const g2 = domainGroups[domains[j]];
        const linkCount = Math.min(5, Math.min(g1.length, g2.length));
        for (let k = 0; k < linkCount; k++) {
          const n1 = g1[Math.floor(Math.random() * g1.length)];
          const n2 = g2[Math.floor(Math.random() * g2.length)];
          const p1 = nodePositions.get(n1);
          const p2 = nodePositions.get(n2);
          if (p1 && p2) {
            addLine(p1, p2, 0.22, 0.74, 0.97, 0.15);
          }
        }
      }
    }

    // Real DB edges
    edges.forEach((edge) => {
      const p1 = nodePositions.get(edge.source);
      const p2 = nodePositions.get(edge.target);
      if (p1 && p2) {
        const strength = Math.min(1, edge.strength);
        addLine(p1, p2, 0.22, 0.74, 0.97, 0.2 + strength * 0.5);
      }
    });

    return {
      positions: new Float32Array(posArr),
      colors: new Float32Array(colArr),
    };
  }, [nodePositions, edges]);

  if (positions.length === 0) return null;

  return (
    <lineSegments frustumCulled={false}>
      <bufferGeometry>
        <bufferAttribute
          attach="attributes-position"
          args={[positions, 3]}
        />
        <bufferAttribute
          attach="attributes-color"
          args={[colors, 3]}
        />
      </bufferGeometry>
      <lineBasicMaterial
        vertexColors
        transparent
        opacity={0.6}
        depthWrite={false}
        blending={THREE.AdditiveBlending}
      />
    </lineSegments>
  );
}
