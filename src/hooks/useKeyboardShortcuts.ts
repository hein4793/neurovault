import { useEffect } from "react";
import { useUiStore } from "@/stores/uiStore";
import { useGraphStore } from "@/stores/graphStore";

export function useKeyboardShortcuts() {
  const { setActivePanel, toggleShortcutHelp } = useUiStore();
  const { selectedNode, selectNode } = useGraphStore();

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      // Ignore if typing in an input/textarea
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;

      const ctrl = e.ctrlKey || e.metaKey;

      if (e.key === "?" && !ctrl) {
        e.preventDefault();
        toggleShortcutHelp();
        return;
      }

      if (e.key === "Escape") {
        e.preventDefault();
        if (selectedNode) {
          selectNode(null);
        } else {
          setActivePanel(null);
        }
        // Also close shortcut help
        useUiStore.getState().showShortcutHelp && toggleShortcutHelp();
        return;
      }

      if (ctrl && e.key === "k") {
        e.preventDefault();
        selectNode(null);
        setActivePanel("search");
        // Focus the search input after a tick
        setTimeout(() => {
          const input = document.querySelector<HTMLInputElement>('input[placeholder*="Search"]');
          input?.focus();
        }, 50);
        return;
      }

      if (ctrl && e.key === "i") {
        e.preventDefault();
        selectNode(null);
        setActivePanel("ingest");
        return;
      }

      if (ctrl && e.key === "r") {
        e.preventDefault();
        selectNode(null);
        setActivePanel("research");
        return;
      }

      if (ctrl && e.key === "d") {
        e.preventDefault();
        selectNode(null);
        setActivePanel("stats");
        return;
      }

      if (ctrl && e.key === ",") {
        e.preventDefault();
        selectNode(null);
        setActivePanel("settings");
        return;
      }

      if (ctrl && e.key === "e") {
        e.preventDefault();
        // Trigger edit mode on selected node - dispatch custom event
        if (selectedNode) {
          window.dispatchEvent(new CustomEvent("brain:edit-node"));
        }
        return;
      }
    };

    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [selectedNode, selectNode, setActivePanel, toggleShortcutHelp]);
}
