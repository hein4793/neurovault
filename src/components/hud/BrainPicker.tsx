import { useEffect, useState } from "react";
import {
  listBrains,
  createBrain,
  getActiveBrain,
  setActiveBrain,
  Brain,
} from "@/lib/tauri";

/**
 * Phase 4.1 — Multi-brain UI picker.
 *
 * Drops into the top HUD as a small dropdown. Lets the user switch
 * between registered brains (work / personal / project-X) and create
 * new ones inline.
 *
 * Backend already exposes list_brains, create_brain, get_active_brain,
 * set_active_brain commands. This component is a thin client over them.
 */
export function BrainPicker() {
  const [brains, setBrains] = useState<Brain[]>([]);
  const [active, setActive] = useState<string>("main");
  const [open, setOpen] = useState(false);
  const [creating, setCreating] = useState(false);
  const [newSlug, setNewSlug] = useState("");
  const [newName, setNewName] = useState("");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    refresh();
  }, []);

  async function refresh() {
    try {
      const [list, current] = await Promise.all([listBrains(), getActiveBrain()]);
      setBrains(list);
      setActive(current);
    } catch (e) {
      console.warn("BrainPicker: failed to load brains", e);
    }
  }

  async function handleSwitch(slug: string) {
    try {
      await setActiveBrain(slug);
      setActive(slug);
      setOpen(false);
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleCreate() {
    setError(null);
    if (!newSlug.trim() || !newName.trim()) {
      setError("Slug and name are required");
      return;
    }
    try {
      await createBrain(newSlug.trim(), newName.trim());
      setCreating(false);
      setNewSlug("");
      setNewName("");
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  }

  const activeBrain = brains.find((b) => b.slug === active);
  const accent = activeBrain?.color || "#00A8FF";

  return (
    <div className="relative">
      <button
        onClick={() => setOpen((o) => !o)}
        className="flex items-center gap-2 px-3 py-1.5 rounded-lg bg-brain-bg/60 border border-brain-border/40 hover:bg-brain-panel/60 transition-colors text-xs font-mono"
        title="Switch brain"
      >
        <span
          className="w-2 h-2 rounded-full"
          style={{ backgroundColor: accent }}
        />
        <span className="text-brain-text">{activeBrain?.name || active}</span>
        <svg
          className={`w-3 h-3 text-brain-muted transition-transform ${open ? "rotate-180" : ""}`}
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
        </svg>
      </button>

      {open && (
        <div className="absolute top-full left-0 mt-1 min-w-[240px] bg-brain-panel border border-brain-border/50 rounded-lg shadow-xl z-50 overflow-hidden">
          {/* Brain list */}
          <div className="max-h-64 overflow-y-auto">
            {brains.map((b) => (
              <button
                key={b.slug}
                onClick={() => handleSwitch(b.slug)}
                className={`w-full px-3 py-2 flex items-center gap-2 text-left hover:bg-brain-bg/40 transition-colors text-xs font-mono ${
                  b.slug === active ? "bg-brain-accent/10" : ""
                }`}
              >
                <span
                  className="w-2 h-2 rounded-full flex-shrink-0"
                  style={{ backgroundColor: b.color }}
                />
                <div className="flex-1 min-w-0">
                  <div className="text-brain-text truncate">{b.name}</div>
                  {b.description && (
                    <div className="text-brain-muted/60 text-[10px] truncate">{b.description}</div>
                  )}
                </div>
                {b.slug === active && (
                  <svg className="w-3 h-3 text-emerald-400 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={3} d="M5 13l4 4L19 7" />
                  </svg>
                )}
              </button>
            ))}
          </div>

          {/* Create new brain */}
          <div className="border-t border-brain-border/30 p-2">
            {!creating ? (
              <button
                onClick={() => setCreating(true)}
                className="w-full px-2 py-1.5 text-xs font-mono text-brain-muted hover:text-brain-accent transition-colors flex items-center gap-2"
              >
                <svg className="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
                </svg>
                Create new brain
              </button>
            ) : (
              <div className="space-y-1.5">
                <input
                  type="text"
                  value={newSlug}
                  onChange={(e) => setNewSlug(e.target.value)}
                  placeholder="slug (e.g. work)"
                  className="w-full text-xs bg-brain-bg/70 border border-brain-border/50 rounded px-2 py-1 text-brain-text font-mono outline-none focus:border-brain-accent/60"
                />
                <input
                  type="text"
                  value={newName}
                  onChange={(e) => setNewName(e.target.value)}
                  placeholder="display name"
                  className="w-full text-xs bg-brain-bg/70 border border-brain-border/50 rounded px-2 py-1 text-brain-text font-mono outline-none focus:border-brain-accent/60"
                />
                {error && (
                  <div className="text-[10px] text-red-400 font-mono px-1">{error}</div>
                )}
                <div className="flex gap-1">
                  <button
                    onClick={handleCreate}
                    className="flex-1 px-2 py-1 text-xs font-mono bg-brain-accent/20 text-brain-accent rounded hover:bg-brain-accent/30 transition-colors"
                  >
                    Create
                  </button>
                  <button
                    onClick={() => {
                      setCreating(false);
                      setError(null);
                      setNewSlug("");
                      setNewName("");
                    }}
                    className="px-2 py-1 text-xs font-mono text-brain-muted hover:text-brain-text transition-colors"
                  >
                    Cancel
                  </button>
                </div>
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
