import { useState, useCallback } from "react";
import { useGraphStore } from "@/stores/graphStore";
import { searchNodes as searchNodesApi, SearchResult } from "@/lib/tauri";

export function useSearch() {
  const [results, setResults] = useState<SearchResult[]>([]);
  const [isSearching, setIsSearching] = useState(false);
  const { setSearchQuery, setHighlightedNodes, selectNode } = useGraphStore();

  const search = useCallback(
    async (query: string) => {
      setSearchQuery(query);

      if (!query.trim()) {
        setResults([]);
        setHighlightedNodes(new Set());
        return;
      }

      setIsSearching(true);
      try {
        const searchResults = await searchNodesApi(query);
        setResults(searchResults);
        setHighlightedNodes(new Set(searchResults.map((r) => r.node.id)));
      } catch (err) {
        console.error("Search failed:", err);
        setResults([]);
      } finally {
        setIsSearching(false);
      }
    },
    [setSearchQuery, setHighlightedNodes]
  );

  const selectResult = useCallback(
    (result: SearchResult) => {
      selectNode(result.node);
    },
    [selectNode]
  );

  return { results, isSearching, search, selectResult };
}
