import { useRef, useMemo } from "react";
import { useFrame } from "@react-three/fiber";
import { OrbitControls, Points, PointMaterial } from "@react-three/drei";
import * as THREE from "three";
import { BRAIN_SCALE } from "@/lib/constants";

export function EnvironmentSetup() {
  return (
    <>
      {/* Background and fog */}
      <color attach="background" args={["#050510"]} />
      <fogExp2 attach="fog" args={["#050510", 0.0015]} />

      {/* Ambient base light */}
      <ambientLight intensity={0.6} color="#1a1a3e" />

      {/* Core light — centered in brain (subtle, not blinding) */}
      <pointLight
        position={[0, BRAIN_SCALE * 0.15, 0]}
        intensity={1.2}
        distance={BRAIN_SCALE * 4}
        color="#38bdf8"
        decay={2}
      />

      {/* Top light */}
      <pointLight
        position={[0, BRAIN_SCALE * 1.2, 0]}
        intensity={1.5}
        distance={BRAIN_SCALE * 3}
        color="#8b5cf6"
        decay={2}
      />

      {/* Bottom light */}
      <pointLight
        position={[0, -BRAIN_SCALE * 0.5, 0]}
        intensity={1.0}
        distance={BRAIN_SCALE * 2}
        color="#0ea5e9"
        decay={2}
      />

      {/* Front fill */}
      <pointLight
        position={[0, BRAIN_SCALE * 0.3, BRAIN_SCALE * 1.5]}
        intensity={0.8}
        distance={BRAIN_SCALE * 3}
        color="#22d3ee"
        decay={2}
      />

      <Starfield />
      <OrbitControls
        enableDamping
        dampingFactor={0.05}
        minDistance={100}
        maxDistance={1200}
        enablePan={false}
        rotateSpeed={0.5}
        zoomSpeed={0.8}
      />
    </>
  );
}

function Starfield() {
  const count = 1500;
  const positions = useMemo(() => {
    const arr = new Float32Array(count * 3);
    for (let i = 0; i < count; i++) {
      // Distribute stars in a large sphere around the brain
      const r = BRAIN_SCALE * 1.8 + Math.random() * BRAIN_SCALE * 6;
      const theta = Math.random() * Math.PI * 2;
      const phi = Math.acos(2 * Math.random() - 1);
      arr[i * 3] = r * Math.sin(phi) * Math.cos(theta);
      arr[i * 3 + 1] = r * Math.sin(phi) * Math.sin(theta);
      arr[i * 3 + 2] = r * Math.cos(phi);
    }
    return arr;
  }, []);

  const ref = useRef<THREE.Points>(null!);
  useFrame((_, delta) => {
    if (ref.current) {
      ref.current.rotation.y += delta * 0.003;
    }
  });

  return (
    <points ref={ref} frustumCulled={false}>
      <bufferGeometry>
        <bufferAttribute
          attach="attributes-position"
          args={[positions, 3]}
        />
      </bufferGeometry>
      <pointsMaterial
        size={1.2}
        transparent
        opacity={0.6}
        color="#4488cc"
        sizeAttenuation
        depthWrite={false}
        blending={THREE.AdditiveBlending}
      />
    </points>
  );
}
