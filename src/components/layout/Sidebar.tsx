import { useUiStore, Panel } from "@/stores/uiStore";
import { useGraphStore } from "@/stores/graphStore";

const navItems: { id: Panel; label: string; color?: string; svg: string }[] = [
  { id: "search", label: "Search (Ctrl+K)", svg: "M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" },
  { id: "ingest", label: "Add Knowledge (Ctrl+I)", svg: "M12 4v16m8-8H4" },
  { id: "research", label: "Research & Learn", svg: "M9.663 17h4.673M12 3v1m6.364 1.636l-.707.707M21 12h-1M4 12H3m3.343-5.657l-.707-.707m2.828 9.9a5 5 0 117.072 0l-.548.547A3.374 3.374 0 0014 18.469V19a2 2 0 11-4 0v-.531c0-.895-.356-1.754-.988-2.386l-.548-.547z" },
  { id: "ask", label: "Ask the Brain", color: "text-brain-accent", svg: "M8 10h.01M12 10h.01M16 10h.01M9 16H5a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v8a2 2 0 01-2 2h-5l-5 5v-5z" },
  { id: "context" as Panel, label: "Brain Sidekick", color: "text-cyan-400", svg: "M13 10V3L4 14h7v7l9-11h-7z" },
  { id: "learning", label: "Autonomous Learning", color: "text-brain-research", svg: "M12 6.253v13m0-13C10.832 5.477 9.246 5 7.5 5S4.168 5.477 3 6.253v13C4.168 18.477 5.754 18 7.5 18s3.332.477 4.5 1.253m0-13C13.168 5.477 14.754 5 16.5 5c1.747 0 3.332.477 4.5 1.253v13C19.832 18.477 18.247 18 16.5 18c-1.746 0-3.332.477-4.5 1.253" },
  { id: "stats", label: "Statistics (Ctrl+D)", svg: "M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z" },
  { id: "quality", label: "Brain Health", color: "text-green-400", svg: "M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" },
  { id: "insights", label: "Brain Insights", color: "text-amber-400", svg: "M13 10V3L4 14h7v7l9-11h-7z" },
  { id: "autonomy", label: "Brain Autonomy", color: "text-emerald-400", svg: "M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" },
  { id: "activity" as Panel, label: "Brain Activity (Live)", color: "text-rose-400", svg: "M3 12h2l3-9 4 18 3-9h6" },
  { id: "backup", label: "Backup & Export", color: "text-cyan-400", svg: "M8 7H5a2 2 0 00-2 2v9a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-3m-1 4l-3 3m0 0l-3-3m3 3V4" },
];

export function Sidebar() {
  const { activePanel, setActivePanel } = useUiStore();
  const { selectNode } = useGraphStore();

  return (
    <div className="w-16 h-screen flex flex-col items-center py-4 gap-1 border-r border-brain-border/30 bg-brain-bg/50 overflow-y-auto">
      {/* Brain logo */}
      <div className="w-10 h-10 mb-3 flex items-center justify-center flex-shrink-0">
        <div className="w-8 h-8 rounded-full bg-gradient-to-br from-brain-accent to-brain-research opacity-80 animate-pulse" />
      </div>

      {/* Nav items */}
      {navItems.map((item) => (
        <button
          key={item.id}
          onClick={() => { selectNode(null); setActivePanel(item.id); }}
          className={`w-10 h-10 rounded-lg flex items-center justify-center transition-all duration-200 group relative flex-shrink-0 ${
            activePanel === item.id
              ? `bg-brain-accent/20 ${item.color || "text-brain-accent"} glow-blue`
              : "text-brain-muted hover:text-brain-text hover:bg-brain-panel/50"
          }`}
          title={item.label}
        >
          <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d={item.svg} />
          </svg>
          <span className="absolute left-14 bg-brain-panel text-brain-text text-xs px-2 py-1 rounded opacity-0 group-hover:opacity-100 transition-opacity pointer-events-none whitespace-nowrap z-50 border border-brain-border/50">
            {item.label}
          </span>
        </button>
      ))}

      <div className="flex-1" />

      {/* Settings */}
      <button
        onClick={() => { selectNode(null); setActivePanel("settings"); }}
        className={`w-10 h-10 rounded-lg flex items-center justify-center transition-all duration-200 group relative flex-shrink-0 ${
          activePanel === "settings" ? "bg-brain-accent/20 text-brain-accent glow-blue" : "text-brain-muted hover:text-brain-text hover:bg-brain-panel/50"
        }`}
        title="Settings (Ctrl+,)"
      >
        <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.066 2.573c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.573 1.066c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.066-2.573c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
        </svg>
        <span className="absolute left-14 bg-brain-panel text-brain-text text-xs px-2 py-1 rounded opacity-0 group-hover:opacity-100 transition-opacity pointer-events-none whitespace-nowrap z-50 border border-brain-border/50">
          Settings
        </span>
      </button>
    </div>
  );
}
