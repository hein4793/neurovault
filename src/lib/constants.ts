import { DOMAIN_COLORS } from "./tauri";

export const BRAIN_SCALE = 180;
export const BRAIN_CENTER_Y = BRAIN_SCALE * 0.15;
export const MAX_VISIBLE_NODES = 600;
export const MAX_VISIBLE_EDGES = 800;

// Skill agent names (top 20 Claude Code expert skills)
export const SKILL_AGENTS = [
  "React", "TypeScript", "Python", "Rust", "Go", "Docker", "PostgreSQL",
  "GraphQL", "Next.js", "Tailwind", "FastAPI", "AWS", "Terraform",
  "Jest", "Playwright", "Vite", "Prisma", "TensorFlow", "LangChain", "Tauri",
];

// Domain color map as RGB floats for Three.js
export const DOMAIN_COLORS_RGB: Record<string, [number, number, number]> = {
  technology: [0.0, 0.66, 1.0],
  business: [0.0, 0.8, 0.53],
  research: [0.55, 0.36, 0.96],
  pattern: [0.96, 0.62, 0.04],
  reference: [0.58, 0.64, 0.72],
  personal: [0.98, 0.45, 0.09],
  core: [0.22, 0.74, 0.97],
};

// Domain label positions in 3D space
export const DOMAIN_LABEL_POSITIONS: Record<string, [number, number, number]> = {
  technology: [80, 60, 0],
  business: [-80, 40, 40],
  research: [0, 90, -40],
  pattern: [-60, -20, 60],
  personal: [60, -10, -60],
  reference: [-40, 70, -30],
};

export { DOMAIN_COLORS };
