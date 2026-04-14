import { useState, useEffect, useRef } from "react";
import { useGraphStore } from "@/stores/graphStore";
import { ingestUrl, ingestText, ingestFiles, importAiMemory, importChatHistory, getDomainColor, DOMAIN_COLORS } from "@/lib/tauri";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";

type Tab = "url" | "text" | "files" | "import";

export function IngestionPanel() {
  const [activeTab, setActiveTab] = useState<Tab>("url");
  const [isProcessing, setIsProcessing] = useState(false);
  const [status, setStatus] = useState<string>("");
  const { addNodes, addNode } = useGraphStore();

  return (
    <div className="p-4 flex flex-col h-full">
      <h2 className="text-lg font-semibold mb-4 flex items-center gap-2">
        <svg className="w-5 h-5 text-brain-accent" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
        </svg>
        Add Knowledge
      </h2>

      {/* Tabs */}
      <div className="flex gap-1 mb-4 bg-brain-bg/50 rounded-lg p-1">
        {(["url", "text", "files", "import"] as Tab[]).map((tab) => (
          <button
            key={tab}
            onClick={() => setActiveTab(tab)}
            className={`flex-1 text-xs font-mono py-1.5 rounded-md transition-all ${
              activeTab === tab
                ? "bg-brain-accent/20 text-brain-accent"
                : "text-brain-muted hover:text-brain-text"
            }`}
          >
            {tab === "url" ? "URL" : tab === "text" ? "Text" : tab === "files" ? "Files" : "Import"}
          </button>
        ))}
      </div>

      {status && (
        <div className="mb-3 text-xs font-mono px-3 py-2 rounded-lg bg-brain-accent/10 text-brain-accent border border-brain-accent/20">
          {status}
        </div>
      )}

      {activeTab === "url" && (
        <UrlIngest
          isProcessing={isProcessing}
          onIngest={async (url) => {
            setIsProcessing(true);
            setStatus("Fetching and processing...");
            try {
              const nodes = await ingestUrl(url);
              addNodes(nodes);
              setStatus(`Added ${nodes.length} knowledge nodes`);
            } catch (err) {
              setStatus(`Error: ${err}`);
            } finally {
              setIsProcessing(false);
            }
          }}
        />
      )}

      {activeTab === "text" && (
        <TextIngest
          isProcessing={isProcessing}
          onIngest={async (title, content, domain, topic) => {
            setIsProcessing(true);
            setStatus("Saving knowledge...");
            try {
              const node = await ingestText({ title, content, domain, topic });
              addNode(node);
              setStatus(`Added: ${node.title}`);
            } catch (err) {
              setStatus(`Error: ${err}`);
            } finally {
              setIsProcessing(false);
            }
          }}
        />
      )}

      {activeTab === "files" && (
        <FileDropZone
          isProcessing={isProcessing}
          onIngest={(paths) => {
            // Queue-based: FileDropZone manages its own processing state.
            // onIngest is called AFTER successful ingestion to update the
            // graph store. We don't set isProcessing here — the queue does.
            setStatus(`Queue processed ${paths.length} file(s)`);
          }}
        />
      )}

      {activeTab === "import" && (
        <ImportPanel
          isProcessing={isProcessing}
          onImport={async () => {
            setIsProcessing(true);
            setStatus("Importing AI memory...");
            try {
              const nodes = await importAiMemory();
              addNodes(nodes);
              setStatus(`Imported ${nodes.length} nodes from AI memory`);
            } catch (err) {
              setStatus(`Error: ${err}`);
            } finally {
              setIsProcessing(false);
            }
          }}
        />
      )}
    </div>
  );
}

function UrlIngest({ isProcessing, onIngest }: { isProcessing: boolean; onIngest: (url: string) => void }) {
  const [url, setUrl] = useState("");

  return (
    <div className="space-y-3">
      <input
        type="url"
        value={url}
        onChange={(e) => setUrl(e.target.value)}
        placeholder="https://example.com/docs"
        className="w-full bg-brain-bg/50 border border-brain-border/50 rounded-lg px-3 py-2 text-sm font-mono text-brain-text placeholder-brain-muted outline-none focus:border-brain-accent/50"
        disabled={isProcessing}
      />
      <button
        onClick={() => url && onIngest(url)}
        disabled={isProcessing || !url}
        className="w-full py-2 rounded-lg bg-brain-accent/20 text-brain-accent text-sm font-mono hover:bg-brain-accent/30 transition-colors disabled:opacity-50 disabled:cursor-not-allowed border border-brain-accent/20"
      >
        {isProcessing ? "Processing..." : "Ingest URL"}
      </button>
    </div>
  );
}

function TextIngest({ isProcessing, onIngest }: { isProcessing: boolean; onIngest: (title: string, content: string, domain: string, topic: string) => void }) {
  const [title, setTitle] = useState("");
  const [content, setContent] = useState("");
  const [domain, setDomain] = useState("technology");
  const [topic, setTopic] = useState("");

  return (
    <div className="space-y-3">
      <input
        type="text"
        value={title}
        onChange={(e) => setTitle(e.target.value)}
        placeholder="Title"
        className="w-full bg-brain-bg/50 border border-brain-border/50 rounded-lg px-3 py-2 text-sm font-mono text-brain-text placeholder-brain-muted outline-none focus:border-brain-accent/50"
      />
      <select
        value={domain}
        onChange={(e) => setDomain(e.target.value)}
        className="w-full bg-brain-bg/50 border border-brain-border/50 rounded-lg px-3 py-2 text-sm font-mono text-brain-text outline-none focus:border-brain-accent/50"
      >
        {Object.keys(DOMAIN_COLORS).map((d) => (
          <option key={d} value={d}>{d}</option>
        ))}
      </select>
      <input
        type="text"
        value={topic}
        onChange={(e) => setTopic(e.target.value)}
        placeholder="Topic (e.g. rust, react, microservices)"
        className="w-full bg-brain-bg/50 border border-brain-border/50 rounded-lg px-3 py-2 text-sm font-mono text-brain-text placeholder-brain-muted outline-none focus:border-brain-accent/50"
      />
      <textarea
        value={content}
        onChange={(e) => setContent(e.target.value)}
        placeholder="Knowledge content..."
        rows={8}
        className="w-full bg-brain-bg/50 border border-brain-border/50 rounded-lg px-3 py-2 text-sm font-mono text-brain-text placeholder-brain-muted outline-none focus:border-brain-accent/50 resize-none"
      />
      <button
        onClick={() => title && content && onIngest(title, content, domain, topic || "general")}
        disabled={isProcessing || !title || !content}
        className="w-full py-2 rounded-lg bg-brain-accent/20 text-brain-accent text-sm font-mono hover:bg-brain-accent/30 transition-colors disabled:opacity-50 disabled:cursor-not-allowed border border-brain-accent/20"
      >
        {isProcessing ? "Saving..." : "Add to Brain"}
      </button>
    </div>
  );
}

/// Queue-based file ingestion — drop files anytime, they accumulate in a
/// visible queue and process sequentially. No more "second drop overwrites
/// the first" problem.
function FileDropZone({ isProcessing: _parentProcessing, onIngest }: { isProcessing: boolean; onIngest: (paths: string[]) => void }) {
  const [isDragging, setIsDragging] = useState(false);
  const [pathInput, setPathInput] = useState("");

  // The ingestion queue. Each entry has a unique ID for reliable tracking.
  type QueueItem = {
    id: number;
    path: string;
    status: "queued" | "processing" | "done" | "failed";
    nodes?: number;
    error?: string;
  };
  const [queue, setQueue] = useState<QueueItem[]>([]);
  const [isProcessingQueue, setIsProcessingQueue] = useState(false);
  const nextIdRef = useRef(1);

  // Add paths to the queue
  const enqueue = (paths: string[]) => {
    setQueue((prev) => {
      const existing = new Set(prev.map((q) => q.path));
      const newItems: QueueItem[] = paths
        .filter((p) => !existing.has(p))
        .map((p) => ({ id: nextIdRef.current++, path: p, status: "queued" as const }));
      return [...prev, ...newItems];
    });
  };

  // Process ONE item at a time — each gets its own real node count
  useEffect(() => {
    const nextQueued = queue.find((q) => q.status === "queued");
    if (!nextQueued || isProcessingQueue) return;

    const processOne = async () => {
      setIsProcessingQueue(true);
      const itemId = nextQueued.id;
      const itemPath = nextQueued.path;

      // Mark this one item as processing
      setQueue((prev) =>
        prev.map((q) =>
          q.id === itemId ? { ...q, status: "processing" as const } : q
        )
      );

      try {
        const nodes = await ingestFiles([itemPath]);
        onIngest([itemPath]);

        // Mark done with its ACTUAL node count
        setQueue((prev) =>
          prev.map((q) =>
            q.id === itemId
              ? { ...q, status: "done" as const, nodes: nodes.length }
              : q
          )
        );
      } catch (err) {
        setQueue((prev) =>
          prev.map((q) =>
            q.id === itemId
              ? { ...q, status: "failed" as const, error: String(err) }
              : q
          )
        );
      } finally {
        setIsProcessingQueue(false);
      }
    };

    processOne();
  }, [queue, isProcessingQueue]);

  // Listen for Tauri native drag-drop events — always ENQUEUES, never overwrites
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    getCurrentWebview().onDragDropEvent((event) => {
      if (event.payload.type === "enter" || event.payload.type === "over") {
        setIsDragging(true);
      } else if (event.payload.type === "drop") {
        setIsDragging(false);
        const paths = event.payload.paths;
        if (paths.length > 0) {
          enqueue(paths);
        }
      } else if (event.payload.type === "leave") {
        setIsDragging(false);
      }
    }).then((fn) => { unlisten = fn; });

    return () => { unlisten?.(); };
  }, []);

  const handleBrowseFiles = async () => {
    const selected = await open({
      multiple: true,
      filters: [{
        name: "Source Files",
        extensions: ["ts", "tsx", "js", "jsx", "py", "rs", "go", "md", "txt", "json", "toml", "yaml", "yml", "css", "html", "sql", "java", "c", "cpp", "h"],
      }],
    });
    if (selected) {
      const paths = Array.isArray(selected) ? selected : [selected];
      if (paths.length > 0) enqueue(paths);
    }
  };

  const handleBrowseFolder = async () => {
    const selected = await open({ directory: true, multiple: true });
    if (selected) {
      const paths = Array.isArray(selected) ? selected : [selected];
      if (paths.length > 0) enqueue(paths);
    }
  };

  const queuedCount = queue.filter((q) => q.status === "queued").length;
  const processingCount = queue.filter((q) => q.status === "processing").length;
  const doneCount = queue.filter((q) => q.status === "done").length;
  const failedCount = queue.filter((q) => q.status === "failed").length;

  return (
    <div className="space-y-3">
      <div className="text-xs text-brain-muted mb-2">
        Drop files/folders anytime — they queue up and process sequentially.
        Duplicates are skipped automatically.
      </div>

      {/* Drop zone — always active, even while processing */}
      <div
        className={`border-2 border-dashed rounded-xl p-5 text-center transition-all ${
          isDragging
            ? "border-brain-accent bg-brain-accent/10 scale-[1.02]"
            : isProcessingQueue
            ? "border-emerald-500/40 bg-emerald-500/5"
            : "border-brain-border/40"
        }`}
      >
        <svg className={`w-8 h-8 mx-auto mb-2 ${isDragging ? "text-brain-accent" : isProcessingQueue ? "text-emerald-400 animate-pulse" : "text-brain-muted"}`} fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M7 16a4 4 0 01-.88-7.903A5 5 0 1115.9 6L16 6a5 5 0 011 9.9M15 13l-3-3m0 0l-3 3m3-3v12" />
        </svg>
        <div className={`text-sm font-mono ${isDragging ? "text-brain-accent" : isProcessingQueue ? "text-emerald-400" : "text-brain-muted"}`}>
          {isDragging ? "Drop to add to queue!" : isProcessingQueue ? "Processing queue..." : "Drag files or folders here"}
        </div>
        <div className="text-[10px] text-brain-muted/50 mt-1">
          Keep dropping — files accumulate in the queue below
        </div>
      </div>

      {/* Browse buttons */}
      <div className="grid grid-cols-2 gap-2">
        <button
          onClick={handleBrowseFiles}
          className="py-2 rounded-lg bg-brain-accent/20 text-brain-accent text-xs font-mono hover:bg-brain-accent/30 transition-colors border border-brain-accent/20"
        >
          Browse Files
        </button>
        <button
          onClick={handleBrowseFolder}
          className="py-2 rounded-lg bg-brain-research/20 text-brain-research text-xs font-mono hover:bg-brain-research/30 transition-colors border border-brain-research/20"
        >
          Browse Folder
        </button>
      </div>

      {/* Queue summary */}
      {queue.length > 0 && (
        <div className="flex items-center justify-between text-[10px] font-mono text-brain-muted px-1">
          <div className="flex gap-3">
            {queuedCount > 0 && <span className="text-amber-400">{queuedCount} queued</span>}
            {processingCount > 0 && <span className="text-cyan-400 animate-pulse">{processingCount} processing</span>}
            {doneCount > 0 && <span className="text-emerald-400">{doneCount} done</span>}
            {failedCount > 0 && <span className="text-red-400">{failedCount} failed</span>}
          </div>
          <button
            onClick={() => setQueue([])}
            className="text-brain-muted/50 hover:text-brain-muted transition-colors"
            title="Clear queue"
          >
            clear
          </button>
        </div>
      )}

      {/* Queue list */}
      {queue.length > 0 && (
        <div className="max-h-32 overflow-y-auto space-y-0.5">
          {queue.map((item, i) => {
            const filename = item.path.split(/[\\/]/).pop() || item.path;
            return (
              <div
                key={i}
                className={`text-[10px] font-mono px-2 py-1 rounded flex items-center justify-between ${
                  item.status === "done"
                    ? "bg-emerald-500/5 text-emerald-400/80"
                    : item.status === "failed"
                    ? "bg-red-500/5 text-red-400/80"
                    : item.status === "processing"
                    ? "bg-cyan-500/5 text-cyan-400"
                    : "text-brain-muted/60"
                }`}
              >
                <span className="truncate flex-1 mr-2" title={item.path}>{filename}</span>
                <span className="flex-shrink-0">
                  {item.status === "queued" && "..."}
                  {item.status === "processing" && <span className="animate-pulse">ingesting</span>}
                  {item.status === "done" && `${item.nodes ?? 0} nodes`}
                  {item.status === "failed" && "failed"}
                </span>
              </div>
            );
          })}
        </div>
      )}

      {/* Manual path input */}
      <div className="text-[10px] text-brain-muted uppercase tracking-wider mt-2 mb-1">Or enter path manually</div>
      <div className="flex gap-2">
        <input
          type="text"
          value={pathInput}
          onChange={(e) => setPathInput(e.target.value)}
          placeholder="C:\Users\...\my-project"
          className="flex-1 bg-brain-bg/50 border border-brain-border/50 rounded-lg px-3 py-2 text-xs font-mono text-brain-text placeholder-brain-muted outline-none focus:border-brain-accent/50"
          onKeyDown={(e) => { if (e.key === "Enter" && pathInput.trim()) { enqueue([pathInput.trim()]); setPathInput(""); } }}
        />
        <button
          onClick={() => { if (pathInput.trim()) { enqueue([pathInput.trim()]); setPathInput(""); } }}
          disabled={!pathInput.trim()}
          className="px-3 py-2 rounded-lg bg-brain-accent/20 text-brain-accent text-xs font-mono hover:bg-brain-accent/30 transition-colors disabled:opacity-50 border border-brain-accent/20"
        >
          Add
        </button>
      </div>
    </div>
  );
}

function ImportPanel({ isProcessing, onImport }: { isProcessing: boolean; onImport: () => void }) {
  const { addNodes } = useGraphStore();
  const [chatStatus, setChatStatus] = useState("");
  const [isChatImporting, setIsChatImporting] = useState(false);

  const handleChatImport = async () => {
    setIsChatImporting(true);
    setChatStatus("Importing 30 chat sessions (65K+ messages)...");
    try {
      const nodes = await importChatHistory();
      addNodes(nodes);
      setChatStatus(`Imported ${nodes.length} conversation neurons from all projects`);
    } catch (err) {
      setChatStatus(`Error: ${err}`);
    } finally {
      setIsChatImporting(false);
    }
  };

  return (
    <div className="space-y-4">
      <div className="text-sm text-brain-muted">
        Import knowledge from AI assistant memory, vault, and chat history.
      </div>

      <div className="space-y-2 text-xs font-mono text-brain-muted">
        <div className="flex items-center gap-2">
          <div className="w-2 h-2 rounded-full bg-brain-tech" />
          AI assistant projects/*/memory/*.md
        </div>
        <div className="flex items-center gap-2">
          <div className="w-2 h-2 rounded-full bg-brain-business" />
          AI assistant vault/**/*.md
        </div>
      </div>

      <button
        onClick={onImport}
        disabled={isProcessing}
        className="w-full py-2 rounded-lg bg-brain-research/20 text-brain-research text-sm font-mono hover:bg-brain-research/30 transition-colors disabled:opacity-50 disabled:cursor-not-allowed border border-brain-research/20"
      >
        {isProcessing ? "Importing..." : "Import AI Memory"}
      </button>

      {/* Chat History Import */}
      <div className="border-t border-brain-border/30 pt-3">
        <div className="text-sm font-semibold text-brain-personal mb-2">Chat History</div>
        <div className="space-y-2 text-xs font-mono text-brain-muted mb-2">
          <div className="flex items-center gap-2">
            <div className="w-2 h-2 rounded-full bg-brain-personal" />
            30 chat sessions across all projects
          </div>
          <div className="flex items-center gap-2">
            <div className="w-2 h-2 rounded-full bg-brain-personal" />
            ProjectA, ProjectB, Notes, Brain...
          </div>
        </div>

        {chatStatus && (
          <div className="mb-2 text-xs font-mono px-3 py-2 rounded-lg bg-brain-personal/10 text-brain-personal border border-brain-personal/20">
            {chatStatus}
          </div>
        )}

        <button
          onClick={handleChatImport}
          disabled={isChatImporting || isProcessing}
          className="w-full py-2 rounded-lg bg-brain-personal/20 text-brain-personal text-sm font-mono hover:bg-brain-personal/30 transition-colors disabled:opacity-50 disabled:cursor-not-allowed border border-brain-personal/20"
        >
          {isChatImporting ? "Importing Chats..." : "Import All Chat History"}
        </button>
      </div>
    </div>
  );
}
