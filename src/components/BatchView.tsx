import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { open as openFileDialog, save as saveFileDialog } from "@tauri-apps/plugin-dialog";
import {
  Activity,
  CheckCircle2,
  Clock,
  Copy,
  Download,
  FileText,
  FolderOpen,
  Languages,
  Layers,
  Loader2,
  Square,
  SquareX,
  Terminal,
  Upload,
} from "lucide-react";
import { toast } from "sonner";
import {
  cancelBatchFolder,
  exportSubtitlesText,
  formatTauriError,
  importVideo,
  mergeBatchSubtitles,
  revealInExplorer,
  saveTextFile,
  scanBatchFolder,
  startBatchFolder,
  stopAndClearQueue,
} from "@/lib/tauri";
import {
  LANGUAGE_OPTIONS,
  type BatchIndexItem,
  type BatchProgressEvent,
  type BatchTimestampMode,
  type PipelineProgressEvent,
  type SubtitleTextFormat,
  type VideoProject,
} from "@/types/studio";
import { Button } from "@/components/ui/button";
import { Badge, Card } from "@/components/ui/card";

interface BatchClip {
  project: VideoProject;
  status: "queued" | "extracting" | "completed" | "failed";
  pct: number;
  stage: string;
  logs: string[];
  srtContent?: string;
  textContent?: string;
}

export function BatchView() {
  const [subTab, setSubTab] = useState<"clips" | "folder">("clips");

  // Multi-clip queue & merge state
  const [selectedFormat, setSelectedFormat] = useState<SubtitleTextFormat>("srt");
  const [selectedLanguage, setSelectedLanguage] = useState("hi");
  const [clips, setClips] = useState<BatchClip[]>([]);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [activeLogId, setActiveLogId] = useState<string | null>(null);
  const [mergedPreview, setMergedPreview] = useState<{
    filename: string;
    content: string;
  } | null>(null);

  // Folder auto-translator state (`batch_transcribe_translate.js` parity)
  const [folderPath, setFolderPath] = useState("");
  const [folderItems, setFolderItems] = useState<BatchIndexItem[]>([]);
  const [timestampMode, setTimestampMode] = useState<BatchTimestampMode>("with");
  const [forceRestart, setForceRestart] = useState(false);
  const [folderRunning, setFolderRunning] = useState(false);
  const [folderProgress, setFolderProgress] = useState<BatchProgressEvent | null>(null);
  const [mergedTxtPath, setMergedTxtPath] = useState<string | null>(null);

  useEffect(() => {
    const unlistenPipe = listen<PipelineProgressEvent>("pipeline://progress", (ev) => {
      const p = ev.payload;
      setClips((prev) =>
        prev.map((c) => {
          if (c.project.id !== p.projectId) return c;
          const done = p.eventType === "done" || p.stage === "TRANSCRIBED";
          const failed = p.eventType === "error";
          const nextLogs = p.logMessage
            ? [...c.logs.slice(-80), `[${new Date().toLocaleTimeString()}] > ${p.logMessage}`]
            : c.logs;
          return {
            ...c,
            pct: p.pct,
            stage: p.stage,
            status: done ? "completed" : failed ? "failed" : "extracting",
            logs: nextLogs,
          };
        }),
      );

      if (p.eventType === "done" || p.stage === "TRANSCRIBED") {
        void Promise.all([
          exportSubtitlesText(p.projectId, "srt"),
          exportSubtitlesText(p.projectId, "text"),
        ]).then(([srt, txt]) => {
          setClips((prev) =>
            prev.map((c) =>
              c.project.id === p.projectId
                ? { ...c, srtContent: srt.content, textContent: txt.content }
                : c,
            ),
          );
        });
      }
    });

    const unlistenBatch = listen<BatchProgressEvent>("batch://progress", (ev) => {
      setFolderProgress(ev.payload);
      if (ev.payload.mergedTxtPath) {
        setMergedTxtPath(ev.payload.mergedTxtPath);
      }
    });

    return () => {
      void unlistenPipe.then((fn) => fn());
      void unlistenBatch.then((fn) => fn());
    };
  }, []);

  const handlePickMultiClips = async () => {
    try {
      const picked = await openFileDialog({
        multiple: true,
        filters: [{ name: "Videos", extensions: ["mp4", "mov", "mkv", "avi", "webm"] }],
      });
      if (!picked) return;
      const paths = Array.isArray(picked) ? picked : [picked];
      const added: BatchClip[] = [];
      for (const path of paths) {
        const proj = await importVideo(path, selectedLanguage, true);
        added.push({
          project: proj,
          status: "queued",
          pct: 5,
          stage: "UPLOADED",
          logs: [`Enqueued ${proj.originalFilename} (${selectedLanguage})`],
        });
      }
      setClips((prev) => [...prev, ...added]);
      setSelectedIds((prev) => {
        const next = new Set(prev);
        added.forEach((a) => next.add(a.project.id));
        return next;
      });
    } catch (err) {
      toast.error(formatTauriError(err));
    }
  };

  const handleMergeSelected = async () => {
    const ids = Array.from(selectedIds);
    if (ids.length === 0) return;
    try {
      const res = await mergeBatchSubtitles(ids, selectedFormat);
      setMergedPreview({ filename: res.filename, content: res.content });
    } catch (err) {
      toast.error(formatTauriError(err));
    }
  };

  const handleSaveMerged = async () => {
    if (!mergedPreview) return;
    const ext = selectedFormat === "srt" ? "srt" : "txt";
    const dest = await saveFileDialog({
      defaultPath: mergedPreview.filename,
      filters: [{ name: ext.toUpperCase(), extensions: [ext] }],
    });
    if (!dest) return;
    await saveTextFile(dest, mergedPreview.content);
    toast.success(`Saved ${mergedPreview.filename}`);
  };

  const handlePickFolder = async () => {
    const picked = await openFileDialog({ directory: true, multiple: false });
    if (!picked || Array.isArray(picked)) return;
    setFolderPath(picked);
    try {
      const items = await scanBatchFolder(picked);
      setFolderItems(items);
    } catch (err) {
      toast.error(formatTauriError(err));
    }
  };

  const handleStartFolderBatch = async () => {
    if (!folderPath) return;
    setFolderRunning(true);
    try {
      const res = await startBatchFolder(folderPath, timestampMode, forceRestart);
      setFolderItems(res.items);
      setMergedTxtPath(res.mergedTxtPath);
      toast.success("Batch folder translation complete!");
    } catch (err) {
      toast.error(formatTauriError(err));
    } finally {
      setFolderRunning(false);
    }
  };

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-xl font-bold text-white flex items-center gap-2">
            <Layers className="w-5 h-5 text-indigo-400" />
            Batch Processing &amp; English Translation
          </h2>
          <p className="text-xs text-zinc-400">
            Multi-clip timeline merge or resumable folder translation (`index.json`)
          </p>
        </div>

        <div className="flex gap-1.5 p-1 rounded-xl bg-zinc-900 border border-zinc-800">
          <Button
            size="sm"
            variant={subTab === "clips" ? "default" : "ghost"}
            onClick={() => setSubTab("clips")}
          >
            Multi-Clip Queue &amp; Merge
          </Button>
          <Button
            size="sm"
            variant={subTab === "folder" ? "default" : "ghost"}
            onClick={() => setSubTab("folder")}
          >
            Folder Auto-Translator
          </Button>
        </div>
      </div>

      {subTab === "clips" && (
        <div className="space-y-4">
          <Card className="p-5 flex flex-wrap items-center justify-between gap-4">
            <div className="flex items-center gap-2">
              <Languages className="w-4 h-4 text-indigo-400" />
              <select
                value={selectedLanguage}
                onChange={(e) => setSelectedLanguage(e.target.value)}
                className="bg-zinc-900 border border-zinc-700 rounded-xl px-3 py-1.5 text-xs text-white"
              >
                {LANGUAGE_OPTIONS.map((o) => (
                  <option key={o.code} value={o.code}>
                    {o.name}
                  </option>
                ))}
              </select>
            </div>

            <div className="flex items-center gap-2">
              <Button
                size="sm"
                variant={selectedFormat === "srt" ? "default" : "secondary"}
                onClick={() => setSelectedFormat("srt")}
              >
                <Clock className="w-3.5 h-3.5" /> SRT
              </Button>
              <Button
                size="sm"
                variant={selectedFormat === "text" ? "default" : "secondary"}
                onClick={() => setSelectedFormat("text")}
              >
                <FileText className="w-3.5 h-3.5" /> Plain Text
              </Button>
              <Button onClick={() => void handlePickMultiClips()}>
                <Upload className="w-3.5 h-3.5" /> Add Video Clips
              </Button>
            </div>
          </Card>

          {clips.length > 0 && (
            <Card className="p-5 space-y-4">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <span className="text-xs font-semibold text-zinc-300 flex items-center gap-2">
                  <Activity className="w-4 h-4 text-indigo-400" />
                  {clips.length} Clips &bull;{" "}
                  {clips.filter((c) => c.status === "completed").length} Completed
                </span>
                <div className="flex gap-2">
                  <Button
                    size="sm"
                    variant="destructive"
                    onClick={() => void stopAndClearQueue()}
                  >
                    <SquareX className="w-3.5 h-3.5" /> Stop Queue
                  </Button>
                  <Button
                    size="sm"
                    disabled={selectedIds.size === 0}
                    onClick={() => void handleMergeSelected()}
                  >
                    <Layers className="w-3.5 h-3.5" /> Merge Selected ({selectedIds.size})
                  </Button>
                </div>
              </div>

              <div className="space-y-2">
                {clips.map((c) => {
                  const content =
                    selectedFormat === "text" ? c.textContent : c.srtContent;
                  const checked = selectedIds.has(c.project.id);
                  return (
                    <div
                      key={c.project.id}
                      className="p-3 rounded-xl bg-zinc-900/70 border border-zinc-800 space-y-2"
                    >
                      <div className="flex flex-wrap items-center justify-between gap-2">
                        <label className="flex items-center gap-2 text-xs font-bold text-white cursor-pointer">
                          <input
                            type="checkbox"
                            checked={checked}
                            onChange={() => {
                              setSelectedIds((prev) => {
                                const next = new Set(prev);
                                if (next.has(c.project.id)) next.delete(c.project.id);
                                else next.add(c.project.id);
                                return next;
                              });
                            }}
                          />
                          {c.project.originalFilename}
                        </label>

                        <div className="flex items-center gap-2">
                          <Badge
                            variant={
                              c.status === "completed"
                                ? "success"
                                : c.status === "failed"
                                  ? "destructive"
                                  : "default"
                            }
                          >
                            {c.status} ({c.pct}%)
                          </Badge>
                          <Button
                            size="sm"
                            variant="ghost"
                            onClick={() =>
                              setActiveLogId(
                                activeLogId === c.project.id ? null : c.project.id,
                              )
                            }
                          >
                            <Terminal className="w-3.5 h-3.5" /> Log
                          </Button>
                          {content && (
                            <Button
                              size="sm"
                              variant="secondary"
                              onClick={() => {
                                void navigator.clipboard.writeText(content);
                                toast.success("Copied to clipboard");
                              }}
                            >
                              <Copy className="w-3.5 h-3.5" /> Copy
                            </Button>
                          )}
                        </div>
                      </div>

                      {activeLogId === c.project.id && (
                        <div className="p-2.5 rounded-lg bg-zinc-950 font-mono text-[11px] text-emerald-400 max-h-32 overflow-y-auto">
                          {c.logs.map((l, i) => (
                            <div key={i}>{l}</div>
                          ))}
                        </div>
                      )}
                    </div>
                  );
                })}
              </div>
            </Card>
          )}

          {mergedPreview && (
            <Card className="p-5 space-y-3">
              <div className="flex items-center justify-between">
                <h4 className="text-sm font-bold text-white">{mergedPreview.filename}</h4>
                <div className="flex gap-2">
                  <Button
                    size="sm"
                    variant="secondary"
                    onClick={() => {
                      void navigator.clipboard.writeText(mergedPreview.content);
                      toast.success("Copied merged subtitles");
                    }}
                  >
                    <Copy className="w-3.5 h-3.5" /> Copy
                  </Button>
                  <Button size="sm" onClick={() => void handleSaveMerged()}>
                    <Download className="w-3.5 h-3.5" /> Save File
                  </Button>
                </div>
              </div>
              <pre className="p-3 rounded-xl bg-zinc-950 border border-zinc-800 text-xs font-mono text-zinc-200 max-h-72 overflow-y-auto whitespace-pre-wrap">
                {mergedPreview.content}
              </pre>
            </Card>
          )}
        </div>
      )}

      {subTab === "folder" && (
        <Card className="p-5 space-y-4">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div className="flex items-center gap-2 flex-1 min-w-[240px]">
              <Button variant="secondary" onClick={() => void handlePickFolder()}>
                <FolderOpen className="w-4 h-4" /> Select Video Folder
              </Button>
              <span className="font-mono text-xs text-zinc-400 truncate">
                {folderPath || "No folder selected"}
              </span>
            </div>

            <div className="flex flex-wrap items-center gap-2">
              <select
                value={timestampMode}
                onChange={(e) => setTimestampMode(e.target.value as BatchTimestampMode)}
                className="bg-zinc-900 border border-zinc-700 rounded-xl px-3 py-1.5 text-xs text-white"
              >
                <option value="with">With Timestamps</option>
                <option value="without">Without Timestamps</option>
                <option value="both">Both (Timed + Full Text)</option>
              </select>

              <label className="flex items-center gap-1.5 text-xs text-zinc-300 cursor-pointer">
                <input
                  type="checkbox"
                  checked={forceRestart}
                  onChange={(e) => setForceRestart(e.target.checked)}
                />
                Force Re-process
              </label>

              {folderRunning ? (
                <Button variant="destructive" onClick={() => void cancelBatchFolder()}>
                  <Square className="w-3.5 h-3.5" /> Stop Batch
                </Button>
              ) : (
                <Button
                  disabled={!folderPath || folderItems.length === 0}
                  onClick={() => void handleStartFolderBatch()}
                >
                  Start Folder Translate
                </Button>
              )}
            </div>
          </div>

          {folderProgress && (
            <div className="p-3 rounded-xl bg-zinc-900 border border-indigo-500/30 flex items-center justify-between text-xs">
              <span className="flex items-center gap-2 text-zinc-200">
                {folderRunning ? (
                  <Loader2 className="w-4 h-4 animate-spin text-indigo-400" />
                ) : (
                  <CheckCircle2 className="w-4 h-4 text-emerald-400" />
                )}
                [{folderProgress.currentIndex}/{folderProgress.total}] {folderProgress.filename}:{" "}
                {folderProgress.logMessage}
              </span>
              {mergedTxtPath && (
                <Button size="sm" variant="secondary" onClick={() => void revealInExplorer(mergedTxtPath)}>
                  Show Merged .txt
                </Button>
              )}
            </div>
          )}

          {folderItems.length > 0 && (
            <div className="max-h-80 overflow-y-auto divide-y divide-zinc-800 border border-zinc-800 rounded-xl">
              {folderItems.map((item) => (
                <div key={item.id} className="p-3 flex items-center justify-between text-xs">
                  <div className="min-w-0">
                    <div className="font-semibold text-white truncate">{item.filename}</div>
                    <div className="text-[11px] text-zinc-500">
                      Lang: {item.detectedLanguage ?? "pending"} &bull; Words: {item.wordCount} &bull; Cues: {item.cueCount}
                    </div>
                  </div>
                  <Badge
                    variant={
                      item.status === "completed"
                        ? "success"
                        : item.status === "failed"
                          ? "destructive"
                          : "default"
                    }
                  >
                    {item.status}
                  </Badge>
                </div>
              ))}
            </div>
          )}
        </Card>
      )}
    </div>
  );
}

