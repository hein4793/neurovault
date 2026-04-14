import { useState } from "react";
import { useUiStore } from "@/stores/uiStore";
import { useGraphStore } from "@/stores/graphStore";

export function SearchBar() {
  const [query, setQuery] = useState("");
  const { setActivePanel } = useUiStore();
  const { setSearchQuery } = useGraphStore();

  const handleSearch = (e: React.FormEvent) => {
    e.preventDefault();
    if (query.trim()) {
      setSearchQuery(query);
      setActivePanel("search");
    }
  };

  return (
    <form onSubmit={handleSearch}>
      <div className="glass-panel glow-blue px-4 py-2 flex items-center gap-3">
        <svg className="w-4 h-4 text-brain-accent" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
        </svg>
        <input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search the brain... (Ctrl+K)"
          className="bg-transparent border-none outline-none text-brain-text placeholder-brain-muted flex-1 text-sm font-mono"
        />
        <span className="text-brain-muted text-xs font-mono">Enter</span>
      </div>
    </form>
  );
}
