import { useEffect, useMemo, useRef } from "react";
import { useFrame } from "@react-three/fiber";
import * as THREE from "three";
import { getNodeCloud } from "@/lib/tauri";
import { useGraphStore } from "@/stores/graphStore";
import { useBrainContainment } from "@/hooks/useBrainContainment";
import { BRAIN_CENTER_Y } from "@/lib/constants";

/**
 * Neural Cloud — 199K+ knowledge nodes as a living neural starfield.
 *
 * Visual design:
 * - Tiny bright sparks (not blobs) — each point individually visible
 * - Strong domain colors: blue, green, purple, amber, teal, orange
 * - Tier 1 (1000 most-used): larger glowing stars
 * - Tier 2 (next 10000): medium bright dots
 * - Tier 3 (188K): tiny subtle background particles
 * - Subtle twinkle animation makes it feel alive
 */

const vertexShader = `
  attribute float size;
  attribute float phase;
  uniform float uTime;
  varying vec3 vColor;
  varying float vAlpha;
  varying float vSize;

  void main() {
    vColor = color;
    vSize = size;

    // Subtle twinkle: each point has a unique phase
    float twinkle = 0.7 + 0.3 * sin(uTime * 1.5 + phase * 6.28);
    vAlpha = twinkle;

    vec4 mvPosition = modelViewMatrix * vec4(position, 1.0);

    // Size based on tier + distance — big enough to see individual nodes
    gl_PointSize = size * (2000.0 / -mvPosition.z);
    gl_PointSize = max(gl_PointSize, 2.5);
    gl_PointSize = min(gl_PointSize, 35.0);

    gl_Position = projectionMatrix * mvPosition;
  }
`;

const fragmentShader = `
  uniform float uOpacity;
  varying vec3 vColor;
  varying float vAlpha;
  varying float vSize;

  void main() {
    float dist = length(gl_PointCoord - vec2(0.5));
    if (dist > 0.5) discard;

    // Sharp bright center with soft glow halo
    float core = 1.0 - smoothstep(0.0, 0.15, dist);    // bright center dot
    float glow = 1.0 - smoothstep(0.1, 0.5, dist);     // soft outer glow
    float alpha = core * 0.9 + glow * 0.15;

    // Tier 1 (large) gets extra glow
    float tierGlow = vSize > 3.0 ? 0.3 : 0.0;
    alpha += tierGlow * glow;

    // Final color: vivid with slight white core for sparkle
    vec3 finalColor = mix(vColor, vec3(1.0), core * 0.4);

    gl_FragColor = vec4(finalColor, alpha * vAlpha * uOpacity * 0.8);
  }
`;

export function NeuronCloud() {
  const cloudData = useGraphStore((s) => s.cloudData);
  const setCloudData = useGraphStore((s) => s.setCloudData);
  const loadPhase = useGraphStore((s) => s.loadPhase);
  const brainMesh = useBrainContainment((s) => s.mesh);
  const brainReady = useBrainContainment((s) => s.ready);
  const materialRef = useRef<THREE.ShaderMaterial>(null!);
  const opacityRef = useRef(0);
  const retryRef = useRef<ReturnType<typeof setTimeout>>(undefined);

  // Fetch cloud data from backend
  useEffect(() => {
    if (cloudData) return;
    if (loadPhase !== "cloud" && loadPhase !== "ready") return;
    let cancelled = false;

    const fetchCloud = async () => {
      try {
        const data = await getNodeCloud();
        if (cancelled) return;
        if (data.count > 0) {
          console.log(`[Cloud] Loaded ${data.count.toLocaleString()} points`);
          setCloudData(data);
        } else {
          retryRef.current = setTimeout(fetchCloud, 10000);
        }
      } catch (err) {
        console.warn("[Cloud] Fetch failed:", err);
        if (!cancelled) retryRef.current = setTimeout(fetchCloud, 10000);
      }
    };

    fetchCloud();
    return () => { cancelled = true; if (retryRef.current) clearTimeout(retryRef.current); };
  }, [cloudData, setCloudData, loadPhase]);

  // Build geometry from brain mesh shape
  const geometry = useMemo(() => {
    if (!cloudData || cloudData.count === 0 || !brainMesh || !brainReady) return null;

    const count = cloudData.count;
    const meshGeo = brainMesh.geometry as THREE.BufferGeometry;
    // Cast through unknown to BufferAttribute — Three.js's typed accessor
    // returns BufferAttribute | InterleavedBufferAttribute, but on a
    // standard mesh.position we know it's the plain BufferAttribute.
    const meshPositions = meshGeo.attributes.position as THREE.BufferAttribute;
    const vertexCount = meshPositions.count;
    const bbox = new THREE.Box3().setFromBufferAttribute(meshPositions);
    const center = new THREE.Vector3();
    bbox.getCenter(center);

    const positions = new Float32Array(count * 3);
    const colors = new Float32Array(cloudData.colors);
    const sizes = new Float32Array(cloudData.sizes);
    const phases = new Float32Array(count); // for twinkle animation

    for (let i = 0; i < count; i++) {
      // Pick 2 random vertices and lerp for smooth distribution
      const v1 = Math.floor(Math.random() * vertexCount);
      const v2 = Math.floor(Math.random() * vertexCount);
      const t = Math.random();

      const sx = meshPositions.getX(v1) * t + meshPositions.getX(v2) * (1 - t);
      const sy = meshPositions.getY(v1) * t + meshPositions.getY(v2) * (1 - t);
      const sz = meshPositions.getZ(v1) * t + meshPositions.getZ(v2) * (1 - t);

      // Surface-biased fill
      const depth = 0.1 + Math.pow(Math.random(), 0.3) * 0.9;

      let px = center.x + (sx - center.x) * depth;
      let py = center.y + (sy - center.y) * depth;
      let pz = center.z + (sz - center.z) * depth;

      const jitter = 10.0 + depth * 15.0;
      px += (Math.random() - 0.5) * jitter;
      py += (Math.random() - 0.5) * jitter;
      pz += (Math.random() - 0.5) * jitter;

      positions[i * 3] = px;
      positions[i * 3 + 1] = py + BRAIN_CENTER_Y;
      positions[i * 3 + 2] = pz;

      phases[i] = Math.random(); // unique twinkle phase
    }

    const geo = new THREE.BufferGeometry();
    geo.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
    geo.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));
    geo.setAttribute("size", new THREE.Float32BufferAttribute(sizes, 1));
    geo.setAttribute("phase", new THREE.Float32BufferAttribute(phases, 1));
    geo.computeBoundingSphere();

    console.log(`[Cloud] Geometry: ${count.toLocaleString()} points with twinkle`);
    return geo;
  }, [cloudData, brainMesh, brainReady]);

  // Animate: fade-in + twinkle
  useFrame((state) => {
    if (!materialRef.current || !geometry) return;
    // Fade in
    if (opacityRef.current < 1.0) {
      opacityRef.current = Math.min(1.0, opacityRef.current + 0.015);
      materialRef.current.uniforms.uOpacity.value = opacityRef.current;
    }
    // Update time for twinkle
    materialRef.current.uniforms.uTime.value = state.clock.elapsedTime;
  });

  if (!geometry) return null;

  return (
    <points frustumCulled={false}>
      <primitive object={geometry} attach="geometry" />
      <shaderMaterial
        ref={materialRef}
        vertexShader={vertexShader}
        fragmentShader={fragmentShader}
        transparent
        depthWrite={false}
        blending={THREE.AdditiveBlending}
        vertexColors
        uniforms={{
          uOpacity: { value: 0 },
          uTime: { value: 0 },
        }}
      />
    </points>
  );
}
