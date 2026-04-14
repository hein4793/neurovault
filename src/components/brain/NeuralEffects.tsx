import { useRef, useMemo } from "react";
import { useFrame } from "@react-three/fiber";
import * as THREE from "three";
import { BRAIN_SCALE, BRAIN_CENTER_Y, DOMAIN_COLORS_RGB } from "@/lib/constants";
import { useSidekickStore } from "@/stores/sidekickStore";
import { getEllipsoidFallback } from "./BrainContainment";

// Ambient neural activity particles
function ActivityCloud() {
  const count = 200;
  const pointsRef = useRef<THREE.Points>(null!);
  const velocitiesRef = useRef<Float32Array>(undefined!);

  const { positions, colors } = useMemo(() => {
    const pos = new Float32Array(count * 3);
    const col = new Float32Array(count * 3);
    const vel = new Float32Array(count * 3);

    const domainCols = Object.values(DOMAIN_COLORS_RGB);

    for (let i = 0; i < count; i++) {
      const [x, y, z] = getEllipsoidFallback();
      pos[i * 3] = x;
      pos[i * 3 + 1] = y;
      pos[i * 3 + 2] = z;

      const c = domainCols[Math.floor(Math.random() * domainCols.length)];
      col[i * 3] = c[0];
      col[i * 3 + 1] = c[1];
      col[i * 3 + 2] = c[2];

      vel[i * 3] = (Math.random() - 0.5) * 0.3;
      vel[i * 3 + 1] = (Math.random() - 0.5) * 0.3;
      vel[i * 3 + 2] = (Math.random() - 0.5) * 0.3;
    }

    velocitiesRef.current = vel;
    return { positions: pos, colors: col };
  }, []);

  useFrame(() => {
    if (!pointsRef.current || !velocitiesRef.current) return;
    const posAttr = pointsRef.current.geometry.attributes.position as THREE.BufferAttribute;
    const vel = velocitiesRef.current;

    // Update 20 particles per frame (throttle for perf)
    const start = Math.floor(Math.random() * (count - 20));
    for (let i = start; i < start + 20; i++) {
      const idx = i * 3;
      posAttr.array[idx] += vel[idx];
      posAttr.array[idx + 1] += vel[idx + 1];
      posAttr.array[idx + 2] += vel[idx + 2];

      // Boundary check — push back toward center if too far
      const dist = Math.sqrt(
        posAttr.array[idx] ** 2 +
        (posAttr.array[idx + 1] - BRAIN_CENTER_Y) ** 2 +
        posAttr.array[idx + 2] ** 2
      );
      if (dist > BRAIN_SCALE * 1.2) {
        vel[idx] *= -0.8;
        vel[idx + 1] *= -0.8;
        vel[idx + 2] *= -0.8;
      }
    }
    posAttr.needsUpdate = true;
  });

  return (
    <points ref={pointsRef} frustumCulled={false}>
      <bufferGeometry>
        <bufferAttribute attach="attributes-position" args={[positions, 3]} />
        <bufferAttribute attach="attributes-color" args={[colors, 3]} />
      </bufferGeometry>
      <pointsMaterial
        size={0.6}
        transparent
        opacity={0.5}
        vertexColors
        sizeAttenuation
        depthWrite={false}
        blending={THREE.AdditiveBlending}
      />
    </points>
  );
}

// Electrical impulse flash system
function ImpulseSystem() {
  const poolSize = 10;
  const meshesRef = useRef<THREE.Mesh[]>([]);
  const stateRef = useRef<{ life: number; maxLife: number; vel: THREE.Vector3 }[]>(
    Array.from({ length: poolSize }, () => ({
      life: 0,
      maxLife: 0,
      vel: new THREE.Vector3(),
    }))
  );

  useFrame(() => {
    const activity = useSidekickStore.getState().vitals.activityLevel;
    const impulseChance = 0.01 + activity * 0.15;

    for (let i = 0; i < poolSize; i++) {
      const mesh = meshesRef.current[i];
      const state = stateRef.current[i];
      if (!mesh) continue;

      if (state.life <= 0) {
        // Dead impulse — chance to respawn
        if (Math.random() < impulseChance) {
          const [x, y, z] = getEllipsoidFallback();
          mesh.position.set(x, y, z);
          state.maxLife = 30 + Math.random() * 30;
          state.life = state.maxLife;
          state.vel.set(
            (Math.random() - 0.5) * 3,
            (Math.random() - 0.5) * 3,
            (Math.random() - 0.5) * 3
          );
        }
      } else {
        // Alive — animate
        state.life--;
        const t = state.life / state.maxLife;
        const scale = t < 0.3 ? t / 0.3 : 1.0 - (1.0 - t) * 0.3;
        mesh.scale.setScalar(scale * 2.5);
        (mesh.material as THREE.MeshBasicMaterial).opacity = t * 0.8;
        mesh.position.add(state.vel);
        state.vel.multiplyScalar(0.96);
      }
    }
  });

  return (
    <>
      {Array.from({ length: poolSize }, (_, i) => (
        <mesh
          key={i}
          ref={(el) => { if (el) meshesRef.current[i] = el; }}
          visible
        >
          <sphereGeometry args={[1.5, 8, 8]} />
          <meshBasicMaterial
            color="#38bdf8"
            transparent
            opacity={0}
            depthWrite={false}
            blending={THREE.AdditiveBlending}
          />
        </mesh>
      ))}
    </>
  );
}

// Orbiting data stream rings
function DataStreams() {
  const ringsRef = useRef<THREE.Mesh[]>([]);
  const configs = useMemo(() => [
    { radius: BRAIN_SCALE * 0.8, color: "#0ea5e9", tiltX: 0.3, tiltY: 0, speed: 0.003 },
    { radius: BRAIN_SCALE * 1.0, color: "#8b5cf6", tiltX: -0.5, tiltY: 0.8, speed: 0.002 },
    { radius: BRAIN_SCALE * 0.6, color: "#22d3ee", tiltX: 0.7, tiltY: -0.3, speed: 0.004 },
  ], []);

  useFrame(() => {
    const activity = useSidekickStore.getState().vitals.activityLevel;
    const speedMul = 1 + activity * 2;
    ringsRef.current.forEach((ring, i) => {
      if (ring) {
        ring.rotation.y += configs[i].speed * speedMul;
      }
    });
  });

  return (
    <group position={[0, BRAIN_CENTER_Y, 0]}>
      {configs.map((cfg, i) => (
        <mesh
          key={i}
          ref={(el) => { if (el) ringsRef.current[i] = el; }}
          rotation={[cfg.tiltX, cfg.tiltY, 0]}
        >
          <torusGeometry args={[cfg.radius, 0.3, 6, 128]} />
          <meshBasicMaterial
            color={cfg.color}
            transparent
            opacity={0.12}
            depthWrite={false}
            blending={THREE.AdditiveBlending}
          />
        </mesh>
      ))}
    </group>
  );
}

// Visual effect consumer — processes queued effects from sidekick events
function EffectConsumer() {
  useFrame(() => {
    const effects = useSidekickStore.getState().consumeEffects();
    // Effects are consumed and visually manifest through:
    // - Impulse system (increased firing rate via vitals.activityLevel)
    // - Breathing animation (via vitals.activityLevel)
    // - Ring speed (via vitals.activityLevel)
    // The activity level boost from events drives all visual responses
    if (effects.length > 0) {
      const store = useSidekickStore.getState();
      const boost = effects.reduce((sum, e) => sum + (e.intensity || 0.1), 0);
      store.setActivityLevel(Math.min(1, store.vitals.activityLevel + boost));
    }
  });

  return null;
}

export function NeuralEffects() {
  return (
    <>
      <ActivityCloud />
      <ImpulseSystem />
      <DataStreams />
      <EffectConsumer />
    </>
  );
}
