import * as THREE from "three";
import { BRAIN_SCALE } from "@/lib/constants";

const _raycaster = new THREE.Raycaster();
const _direction = new THREE.Vector3(1, 0, 0);
const _origin = new THREE.Vector3();

/**
 * Test if a point is inside the brain mesh using raycasting.
 * Cast a ray in +X direction and count intersections — odd = inside.
 */
export function isInsideBrain(
  x: number, y: number, z: number,
  mesh: THREE.Mesh
): boolean {
  _origin.set(x, y, z);
  _raycaster.set(_origin, _direction);
  const hits = _raycaster.intersectObject(mesh);
  return hits.length % 2 === 1;
}

/**
 * Generate a position guaranteed to be inside the brain mesh.
 * Uses rejection sampling within the mesh bounding box.
 * Falls back to ellipsoid if mesh is not available.
 */
export function getBrainContainedPosition(
  mesh: THREE.Mesh | null,
  bbox: THREE.Box3 | null
): [number, number, number] {
  if (!mesh || !bbox) {
    return getEllipsoidFallback();
  }

  const min = bbox.min;
  const max = bbox.max;
  // Shrink bbox by 5% margin for cleaner containment
  const mx = (max.x - min.x) * 0.025;
  const my = (max.y - min.y) * 0.025;
  const mz = (max.z - min.z) * 0.025;

  for (let attempt = 0; attempt < 50; attempt++) {
    const x = (min.x + mx) + Math.random() * (max.x - min.x - mx * 2);
    const y = (min.y + my) + Math.random() * (max.y - min.y - my * 2);
    const z = (min.z + mz) + Math.random() * (max.z - min.z - mz * 2);
    if (isInsideBrain(x, y, z, mesh)) {
      return [x, y, z];
    }
  }

  // Fallback: center of brain
  return [0, BRAIN_SCALE * 0.15, 0];
}

/**
 * Ellipsoid fallback when brain mesh is not yet loaded.
 */
export function getEllipsoidFallback(): [number, number, number] {
  const r = Math.cbrt(Math.random());
  const theta = Math.random() * Math.PI * 2;
  const phi = Math.acos(2 * Math.random() - 1);
  const rx = BRAIN_SCALE * 0.9;
  const ry = BRAIN_SCALE * 0.65;
  const rz = BRAIN_SCALE * 0.8;
  return [
    r * Math.sin(phi) * Math.cos(theta) * rx,
    r * Math.sin(phi) * Math.sin(theta) * ry + BRAIN_SCALE * 0.15,
    r * Math.cos(phi) * rz,
  ];
}
