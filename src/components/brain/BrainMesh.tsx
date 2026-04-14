import { useRef, useEffect, useMemo } from "react";
import { useFrame } from "@react-three/fiber";
import { useGLTF } from "@react-three/drei";
import * as THREE from "three";
import { BRAIN_SCALE, BRAIN_CENTER_Y } from "@/lib/constants";
import { useBrainContainment } from "@/hooks/useBrainContainment";
import { useSidekickStore } from "@/stores/sidekickStore";

export function BrainMesh() {
  const groupRef = useRef<THREE.Group>(null!);
  const { scene } = useGLTF("/brain-model.glb");
  const setMesh = useBrainContainment((s) => s.setMesh);

  // Extract and scale the brain geometry
  const { solidMesh, wireframeMesh, glowMesh, edgeMesh } = useMemo(() => {
    let geometry: THREE.BufferGeometry | null = null;

    scene.traverse((child) => {
      if ((child as THREE.Mesh).isMesh) {
        geometry = (child as THREE.Mesh).geometry.clone();
      }
    });

    if (!geometry) {
      // Fallback sphere if GLB has no mesh
      geometry = new THREE.SphereGeometry(BRAIN_SCALE, 32, 32);
    }

    // Scale to match BRAIN_SCALE * 3.0
    geometry.computeBoundingBox();
    const box = geometry.boundingBox!;
    const size = new THREE.Vector3();
    box.getSize(size);
    const maxDim = Math.max(size.x, size.y, size.z);
    const scale = (BRAIN_SCALE * 3.0) / maxDim;
    geometry.scale(scale, scale, scale);

    // Center
    geometry.computeBoundingBox();
    const center = new THREE.Vector3();
    geometry.boundingBox!.getCenter(center);
    geometry.translate(-center.x, -center.y, -center.z);
    geometry.computeVertexNormals();

    // Solid holographic brain
    const solid = new THREE.Mesh(
      geometry,
      new THREE.MeshPhongMaterial({
        color: 0x1a3a5c,
        transparent: true,
        opacity: 0.08,
        side: THREE.DoubleSide,
        depthWrite: false,
        blending: THREE.AdditiveBlending,
        shininess: 100,
        specular: new THREE.Color(0x38bdf8),
      })
    );

    // Wireframe overlay
    const wireframe = new THREE.Mesh(
      geometry,
      new THREE.MeshBasicMaterial({
        color: 0x38bdf8,
        wireframe: true,
        transparent: true,
        opacity: 0.12,
        depthWrite: false,
        blending: THREE.AdditiveBlending,
      })
    );

    // Outer glow
    const glow = new THREE.Mesh(
      geometry.clone().scale(1.03, 1.03, 1.03),
      new THREE.MeshBasicMaterial({
        color: 0x0ea5e9,
        transparent: true,
        opacity: 0.03,
        side: THREE.BackSide,
        depthWrite: false,
        blending: THREE.AdditiveBlending,
      })
    );

    // Edge highlight
    const edge = new THREE.Mesh(
      geometry.clone().scale(1.01, 1.01, 1.01),
      new THREE.MeshPhongMaterial({
        color: 0x38bdf8,
        transparent: true,
        opacity: 0.04,
        side: THREE.FrontSide,
        depthWrite: false,
        blending: THREE.AdditiveBlending,
        emissive: new THREE.Color(0x1a6a9a),
        emissiveIntensity: 0.3,
      })
    );

    return { solidMesh: solid, wireframeMesh: wireframe, glowMesh: glow, edgeMesh: edge };
  }, [scene]);

  // Register brain mesh for containment raycasting
  useEffect(() => {
    if (solidMesh) {
      // Update world matrix so raycasting works
      solidMesh.updateMatrixWorld(true);
      setMesh(solidMesh);
    }
  }, [solidMesh, setMesh]);

  // Breathing animation driven by brain vitals
  useFrame((state) => {
    if (!groupRef.current) return;
    const t = state.clock.elapsedTime;
    const activityLevel = useSidekickStore.getState().vitals.activityLevel;

    // Breathing: amplitude and speed scale with activity
    const amplitude = 0.003 + activityLevel * 0.012;
    const speed = 0.4 + activityLevel * 0.6;
    const breathe = 1.0 + Math.sin(t * speed) * amplitude;
    groupRef.current.scale.setScalar(breathe);
  });

  return (
    <group ref={groupRef} position={[0, BRAIN_CENTER_Y, 0]}>
      <primitive object={solidMesh} />
      <primitive object={wireframeMesh} />
      <primitive object={glowMesh} />
      <primitive object={edgeMesh} />
    </group>
  );
}

// Preload the brain model
useGLTF.preload("/brain-model.glb");
