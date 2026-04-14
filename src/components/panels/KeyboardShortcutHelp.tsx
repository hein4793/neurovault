import { useUiStore } from "@/stores/uiStore";

const shortcuts = [
  { keys: "Ctrl+K", action: "Search the brain" },
  { keys: "Ctrl+I", action: "Add knowledge" },
  { keys: "Ctrl+R", action: "Research & Learn" },
  { keys: "Ctrl+D", action: "Statistics dashboard" },
  { keys: "Ctrl+,", action: "Settings" },
  { keys: "Ctrl+E", action: "Edit selected node" },
  { keys: "Escape", action: "Close panel / Deselect" },
  { keys: "?", action: "Toggle this help" },
];

export function KeyboardShortcutHelp() {
  const { showShortcutHelp, toggleShortcutHelp } = useUiStore();

  if (!showShortcutHelp) return null;

  return (
    <div
      className="fixed inset-0 z-[100] flex items-center justify-center bg-brain-bg/60 backdrop-blur-sm"
      onClick={toggleShortcutHelp}
    >
      <div
        className="glass-panel glow-blue p-6 w-[360px] animate-[fade-in_0.15s_ease-out]"
        onClick={(e) => e.stopPropagation()}
      >
        <h2 className="text-sm font-semibold text-brain-accent mb-4 font-mono uppercase tracking-wider">
          Keyboard Shortcuts
        </h2>
        <div className="space-y-2">
          {shortcuts.map((s) => (
            <div key={s.keys} className="flex items-center justify-between">
              <span className="text-xs text-brain-muted font-mono">{s.action}</span>
              <kbd className="text-xs px-2 py-0.5 rounded bg-brain-bg/80 border border-brain-border/50 text-brain-accent font-mono">
                {s.keys}
              </kbd>
            </div>
          ))}
        </div>
        <div className="mt-4 text-center">
          <span className="text-[10px] text-brain-muted/50 font-mono">Press ? or Escape to close</span>
        </div>
      </div>
    </div>
  );
}
