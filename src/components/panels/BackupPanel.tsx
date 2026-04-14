import { useState, useEffect } from "react";
import {
  createBackup,
  listBackups,
  restoreBackup,
  exportJson,
  exportMarkdown,
  exportCsv,
  getSettings,
  BackupInfo,
} from "@/lib/tauri";

async function getExportPath(): Promise<string> {
  try {
    const settings = await getSettings();
    if (settings.data_dir) return `${settings.data_dir}/exports`;
  } catch {}
  return "exports";
}

export function BackupPanel() {
  const [backups, setBackups] = useState<BackupInfo[]>([]);
  const [status, setStatus] = useState("");
  const [isProcessing, setIsProcessing] = useState(false);

  useEffect(() => {
    loadBackups();
  }, []);

  const loadBackups = async () => {
    try {
      const b = await listBackups();
      setBackups(b);
    } catch {}
  };

  const handleBackup = async () => {
    setIsProcessing(true);
    setStatus("Creating backup...");
    try {
      const info = await createBackup();
      setStatus(`Backup created: ${info.filename} (${formatBytes(info.size_bytes)})`);
      loadBackups();
    } catch (err) {
      setStatus(`Error: ${err}`);
    } finally {
      setIsProcessing(false);
    }
  };

  const handleRestore = async (path: string) => {
    setIsProcessing(true);
    setStatus("Restoring backup...");
    try {
      const [nodes, edges] = await restoreBackup(path);
      setStatus(`Restored: ${nodes} nodes, ${edges} edges`);
    } catch (err) {
      setStatus(`Error: ${err}`);
    } finally {
      setIsProcessing(false);
    }
  };

  const handleExport = async (format: string) => {
    setIsProcessing(true);
    const basePath = await getExportPath();
    setStatus(`Exporting as ${format}...`);
    try {
      let count = 0;
      if (format === "json") {
        count = await exportJson(`${basePath}/brain_export.json`);
      } else if (format === "markdown") {
        count = await exportMarkdown(`${basePath}/markdown`);
      } else if (format === "csv") {
        count = await exportCsv(`${basePath}/brain_export.csv`);
      }
      setStatus(`Exported ${count} nodes as ${format}`);
    } catch (err) {
      setStatus(`Error: ${err}`);
    } finally {
      setIsProcessing(false);
    }
  };

  return (
    <div className="p-4 flex flex-col h-full">
      <h2 className="text-lg font-semibold mb-4 flex items-center gap-2 text-brain-text">
        <svg className="w-5 h-5 text-cyan-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 7H5a2 2 0 00-2 2v9a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-3m-1 4l-3 3m0 0l-3-3m3 3V4" />
        </svg>
        Backup & Export
      </h2>

      {status && (
        <div className="mb-3 text-xs font-mono px-3 py-2 rounded-lg bg-cyan-500/10 text-cyan-400 border border-cyan-500/20">
          {status}
        </div>
      )}

      <div className="flex-1 overflow-y-auto space-y-4 min-h-0">
        {/* Backup */}
        <section>
          <h3 className="text-xs font-semibold text-brain-muted uppercase tracking-wider mb-2">Backup</h3>
          <button
            onClick={handleBackup}
            disabled={isProcessing}
            className="w-full py-2.5 rounded-lg bg-gradient-to-r from-cyan-500/20 to-blue-500/20 text-brain-text text-sm font-mono hover:from-cyan-500/30 hover:to-blue-500/30 transition-all disabled:opacity-50 border border-cyan-500/20"
          >
            {isProcessing ? "Processing..." : "Create Backup Now"}
          </button>
        </section>

        {/* Export */}
        <section>
          <h3 className="text-xs font-semibold text-brain-muted uppercase tracking-wider mb-2">Export</h3>
          <div className="grid grid-cols-3 gap-2">
            {["json", "markdown", "csv"].map((fmt) => (
              <button
                key={fmt}
                onClick={() => handleExport(fmt)}
                disabled={isProcessing}
                className="py-2 rounded-lg bg-brain-bg/50 text-brain-muted text-xs font-mono hover:text-brain-text hover:bg-brain-panel/50 transition-colors border border-brain-border/30 disabled:opacity-50 uppercase"
              >
                {fmt}
              </button>
            ))}
          </div>
        </section>

        {/* Backup History */}
        <section>
          <h3 className="text-xs font-semibold text-brain-muted uppercase tracking-wider mb-2">
            Backup History ({backups.length})
          </h3>
          <div className="space-y-1.5">
            {backups.map((b, i) => (
              <div key={i} className="flex items-center justify-between text-xs font-mono px-3 py-2 rounded-lg bg-brain-bg/50 border border-brain-border/30 group">
                <div>
                  <div className="text-brain-text">{b.filename}</div>
                  <div className="text-[10px] text-brain-muted/50">{formatBytes(b.size_bytes)}</div>
                </div>
                <button
                  onClick={() => handleRestore(b.path)}
                  disabled={isProcessing}
                  className="opacity-0 group-hover:opacity-100 text-[10px] px-2 py-0.5 rounded bg-cyan-500/20 text-cyan-400 hover:bg-cyan-500/30 transition-all disabled:opacity-30"
                >
                  Restore
                </button>
              </div>
            ))}
            {backups.length === 0 && (
              <div className="text-center text-brain-muted/50 text-xs font-mono py-3">No backups yet</div>
            )}
          </div>
        </section>
      </div>
    </div>
  );
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
