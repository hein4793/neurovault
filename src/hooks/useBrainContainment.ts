import { create } from "zustand";
import * as THREE from "three";
import { getBrainContainedPosition } from "@/components/brain/BrainContainment";

interface BrainContainmentState {
  mesh: THREE.Mesh | null;
  bbox: THREE.Box3 | null;
  ready: boolean;
  setMesh: (mesh: THREE.Mesh) => void;
  getPosition: () => [number, number, number];
}

export const useBrainContainment = create<BrainContainmentState>((set, get) => ({
  mesh: null,
  bbox: null,
  ready: false,

  setMesh: (mesh: THREE.Mesh) => {
    const bbox = new THREE.Box3().setFromObject(mesh);
    set({ mesh, bbox, ready: true });
  },

  getPosition: () => {
    const { mesh, bbox } = get();
    return getBrainContainedPosition(mesh, bbox);
  },
}));
