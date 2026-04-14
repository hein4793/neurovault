import { useState, useEffect, useRef } from "react";
import { useSidekickStore } from "@/stores/sidekickStore";
import { useUiStore } from "@/stores/uiStore";

interface Suggestion {
  id: number;
  message: string;
  detail?: string;
  panel?: string;
}

export function SuggestionToast() {
  const vitals = useSidekickStore((s) => s.vitals);
  const [visible, setVisible] = useState(false);
  const [suggestion, setSuggestion] = useState<Suggestion | null>(null);
  const setActivePanel = useUiStore((s) => s.setActivePanel);
  const lastIqRef = useRef(vitals.currentIq);
  const timerRef = useRef<ReturnType<typeof setTimeout>>(undefined);

  // Watch for IQ changes
  useEffect(() => {
    if (vitals.currentIq > 0 && vitals.currentIq !== lastIqRef.current) {
      const diff = vitals.currentIq - lastIqRef.current;
      lastIqRef.current = vitals.currentIq;

      if (diff > 0) {
        showSuggestion({
          id: Date.now(),
          message: `Brain IQ increased to ${vitals.currentIq}`,
          detail: `+${diff} points from recent learning`,
          panel: "stats",
        });
      }
    }
  }, [vitals.currentIq]);

  const showSuggestion = (s: Suggestion) => {
    setSuggestion(s);
    setVisible(true);
    // Auto-dismiss
    if (timerRef.current) clearTimeout(timerRef.current);
    timerRef.current = setTimeout(() => setVisible(false), 10_000);
  };

  const dismiss = () => {
    setVisible(false);
    if (timerRef.current) clearTimeout(timerRef.current);
  };

  const explore = () => {
    if (suggestion?.panel) {
      setActivePanel(suggestion.panel as any);
    }
    dismiss();
  };

  if (!visible || !suggestion) return null;

  return (
    <div className="glass-panel w-[280px] p-3 animate-in slide-in-from-right duration-300">
      <div className="flex items-start justify-between mb-1">
        <span className="text-[9px] font-mono text-brain-accent uppercase">Brain Insight</span>
        <button
          onClick={dismiss}
          className="text-brain-muted hover:text-brain-text text-xs leading-none"
        >
          &times;
        </button>
      </div>
      <p className="text-xs font-mono text-brain-text mb-1">{suggestion.message}</p>
      {suggestion.detail && (
        <p className="text-[10px] font-mono text-brain-muted mb-2">{suggestion.detail}</p>
      )}
      {suggestion.panel && (
        <button
          onClick={explore}
          className="text-[10px] font-mono text-brain-accent hover:text-brain-accent/80 transition-colors"
        >
          Explore →
        </button>
      )}
    </div>
  );
}
