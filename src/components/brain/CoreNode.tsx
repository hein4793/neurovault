import { useRef } from "react";
import { useFrame } from "@react-three/fiber";
import * as THREE from "three";
import { BRAIN_CENTER_Y } from "@/lib/constants";
import { useSidekickStore } from "@/stores/sidekickStore";
import { useGraphStore } from "@/stores/graphStore";

export function CoreNode() {
  const groupRef = useRef<THREE.Group>(null!);
  const ringsRef = useRef<THREE.Mesh[]>([]);
  const selectNode = useGraphStore((s) => s.selectNode);

  useFrame((state) => {
    if (!groupRef.current) return;
    const t = state.clock.elapsedTime;
    const activity = useSidekickStore.getState().vitals.activityLevel;
    const speedMul = 1 + activity * 2;

    // Rotate energy rings
    ringsRef.current.forEach((ring, i) => {
      if (ring) {
        ring.rotation.z += (0.008 + i * 0.003) * speedMul;
        ring.rotation.x += (0.002 + i * 0.001) * speedMul;
      }
    });

    // Core pulse
    const pulse = 1.0 + Math.sin(t * 2.5) * 0.1;
    groupRef.current.scale.setScalar(pulse);
  });

  const ringConfigs = [
    { radius: 15, color: "#38bdf8", tiltX: 0.3, tiltZ: 0 },
    { radius: 20, color: "#8b5cf6", tiltX: -0.5, tiltZ: 0.8 },
    { radius: 25, color: "#0ea5e9", tiltX: 0.7, tiltZ: -0.4 },
  ];

  return (
    <group
      ref={groupRef}
      position={[0, BRAIN_CENTER_Y, 0]}
      onClick={(e) => {
        e.stopPropagation();
        selectNode(null);
      }}
    >
      {/* Inner core sphere */}
      <mesh>
        <sphereGeometry args={[8, 16, 16]} />
        <meshPhongMaterial
          color="#38bdf8"
          emissive="#1a5a8a"
          emissiveIntensity={0.3}
          transparent
          opacity={0.25}
          depthWrite={false}
        />
      </mesh>

      {/* Outer glow */}
      <mesh>
        <sphereGeometry args={[14, 16, 16]} />
        <meshBasicMaterial
          color="#0ea5e9"
          transparent
          opacity={0.06}
          side={THREE.BackSide}
          depthWrite={false}
          blending={THREE.AdditiveBlending}
        />
      </mesh>

      {/* Energy rings */}
      {ringConfigs.map((cfg, i) => (
        <mesh
          key={i}
          ref={(el) => { if (el) ringsRef.current[i] = el; }}
          rotation={[cfg.tiltX, 0, cfg.tiltZ]}
        >
          <torusGeometry args={[cfg.radius, 0.4, 8, 64]} />
          <meshBasicMaterial
            color={cfg.color}
            transparent
            opacity={0.3}
            depthWrite={false}
            blending={THREE.AdditiveBlending}
          />
        </mesh>
      ))}
    </group>
  );
}
