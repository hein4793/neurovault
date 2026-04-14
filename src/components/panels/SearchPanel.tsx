import { useState, useEffect } from "react";
import { useSearch } from "@/hooks/useSearch";
import { useGraphStore } from "@/stores/graphStore";
import { getDomainColor } from "@/lib/tauri";
import { truncate } from "@/lib/utils";

export function SearchPanel() {
  const [query, setQuery] = useState("");
  const { results, isSearching, search, selectResult } = useSearch();
  const { searchQuery } = useGraphStore();

  useEffect(() => {
    if (searchQuery && searchQuery !== query) {
      setQuery(searchQuery);
      search(searchQuery);
    }
  }, [searchQuery]);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    search(query);
  };

  return (
    <div className="p-4 flex flex-col h-full">
      <h2 className="text-lg font-semibold mb-4 flex items-center gap-2">
        <svg className="w-5 h-5 text-brain-accent" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
        </svg>
        Search Brain
      </h2>

      <form onSubmit={handleSubmit} className="mb-4">
        <input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search knowledge..."
          className="w-full bg-brain-bg/50 border border-brain-border/50 rounded-lg px-3 py-2 text-sm font-mono text-brain-text placeholder-brain-muted outline-none focus:border-brain-accent/50 transition-colors"
          autoFocus
        />
      </form>

      {isSearching && (
        <div className="text-center text-brain-muted text-sm py-4">Searching...</div>
      )}

      <div className="flex-1 overflow-y-auto space-y-2">
        {results.map((result) => {
          const color = getDomainColor(result.node.domain);
          return (
            <button
              key={result.node.id}
              onClick={() => selectResult(result)}
              className="w-full text-left p-3 rounded-lg bg-brain-bg/30 border border-brain-border/30 hover:border-brain-accent/30 transition-all group"
            >
              <div className="flex items-center gap-2 mb-1">
                <div
                  className="w-2 h-2 rounded-full"
                  style={{ backgroundColor: color }}
                />
                <span className="text-xs text-brain-muted font-mono uppercase">
                  {result.node.domain}
                </span>
                <span className="text-xs text-brain-muted/50 font-mono ml-auto">
                  {Math.round(result.score * 100)}%
                </span>
              </div>
              <div className="text-sm font-medium text-brain-text group-hover:text-brain-accent transition-colors">
                {result.node.title}
              </div>
              <div className="text-xs text-brain-muted mt-1 line-clamp-2">
                {truncate(result.node.summary, 120)}
              </div>
            </button>
          );
        })}

        {!isSearching && query && results.length === 0 && (
          <div className="text-center text-brain-muted text-sm py-8">
            No results found for "{query}"
          </div>
        )}
      </div>
    </div>
  );
}
