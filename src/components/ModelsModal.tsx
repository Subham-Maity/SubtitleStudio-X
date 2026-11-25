import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { CheckCircle2, Cpu, Download, Loader2, Trash2, X } from "lucide-react";
import { toast } from "sonner";
import {
  downloadWhisperModel,
  formatTauriError,
  listWhisperModels,
  revealInExplorer,
  setPreferredModel,
  unloadWhisperModel,
} from "@/lib/tauri";
import type { ModelDownloadProgress } from "@/types/studio";
import { Button } from "@/components/ui/button";
import { Badge, Card } from "@/components/ui/card";

interface ModelsModalProps {
  open: boolean;
  onClose: () => void;
}

export function ModelsModal({ open, onClose }: ModelsModalProps) {
  const queryClient = useQueryClient();
  const [downloadingId, setDownloadingId] = useState<string | null>(null);
  const [progress, setProgress] = useState<ModelDownloadProgress | null>(null);

  const { data: models = [], isLoading } = useQuery({
    queryKey: ["whisper-models"],
    queryFn: listWhisperModels,
    enabled: open,
  });

  useEffect(() => {
    if (!open) return;
    const unlisten = listen<ModelDownloadProgress>("model://progress", (ev) => {
      setProgress(ev.payload);
      if (ev.payload.done) {
        setDownloadingId(null);
        void queryClient.invalidateQueries({ queryKey: ["whisper-models"] });
      }
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [open, queryClient]);

  if (!open) return null;

  const handleDownload = async (modelId: string) => {
    setDownloadingId(modelId);
    setProgress({
      modelId,
      downloadedBytes: 0,
      totalBytes: null,
      pct: 0,
      done: false,
    });
    try {
      await downloadWhisperModel(modelId);
      await setPreferredModel(modelId);
      toast.success(`Model ${modelId} ready`);
      await queryClient.invalidateQueries({ queryKey: ["whisper-models"] });
    } catch (err) {
      toast.error(formatTauriError(err, "Model download failed"));
    } finally {
      setDownloadingId(null);
    }
  };

  const handleUseModel = async (modelId: string) => {
    try {
      await setPreferredModel(modelId);
      toast.success(`Active Whisper model set to ${modelId}`);
    } catch (err) {
      toast.error(formatTauriError(err));
    }
  };

  const handleUnload = async () => {
    try {
      await unloadWhisperModel();
      toast.success("Whisper model unloaded from VRAM/RAM");
    } catch (err) {
      toast.error(formatTauriError(err));
    }
  };

  return (
    <div className="fixed inset-0 z-50 bg-black/75 backdrop-blur-md flex items-center justify-center p-4">
      <Card className="max-w-xl w-full p-6 space-y-5 bg-zinc-950 border-zinc-800 relative">
        <button
          type="button"
          onClick={onClose}
          className="absolute top-4 right-4 p-1.5 rounded-lg text-zinc-400 hover:text-white hover:bg-zinc-800 cursor-pointer"
        >
          <X className="w-4 h-4" />
        </button>

        <div className="flex items-center gap-3">
          <div className="p-2.5 rounded-xl bg-indigo-500/15 border border-indigo-500/30 text-indigo-400">
            <Cpu className="w-5 h-5" />
          </div>
          <div>
            <h3 className="text-base font-bold text-white">Offline Whisper GGML Models</h3>
            <p className="text-xs text-zinc-400">
              Downloaded once to <span className="font-mono text-zinc-300">%APPDATA%\SubtitleStudio\models</span>
            </p>
          </div>
        </div>

        {isLoading ? (
          <div className="py-8 flex items-center justify-center text-xs text-zinc-400 gap-2">
            <Loader2 className="w-4 h-4 animate-spin text-indigo-400" />
            Checking local models...
          </div>
        ) : (
          <div className="space-y-2.5">
            {models.map((m) => {
              const isDl = downloadingId === m.id;
              const pct = isDl && progress?.modelId === m.id ? progress.pct : 0;
              return (
                <div
                  key={m.id}
                  className="p-3.5 rounded-xl bg-zinc-900/70 border border-zinc-800 space-y-2"
                >
                  <div className="flex items-center justify-between gap-3">
                    <div className="min-w-0">
                      <div className="flex items-center gap-2">
                        <span className="text-xs font-bold text-white truncate">{m.label}</span>
                        <Badge variant="default">{m.approxSizeMb} MB</Badge>
                        <Badge variant="warning">{m.vramNote}</Badge>
                      </div>
                      <p className="text-[11px] font-mono text-zinc-500 truncate">{m.fileName}</p>
                    </div>

                    {m.downloaded ? (
                      <div className="flex items-center gap-1.5 shrink-0">
                        <Badge variant="success">
                          <CheckCircle2 className="w-3 h-3" /> Ready
                        </Badge>
                        <Button size="sm" variant="secondary" onClick={() => void handleUseModel(m.id)}>
                          Use
                        </Button>
                        <Button
                          size="sm"
                          variant="ghost"
                          onClick={() => void revealInExplorer(m.localPath)}
                          title="Show in Explorer"
                        >
                          Folder
                        </Button>
                      </div>
                    ) : (
                      <Button
                        size="sm"
                        disabled={downloadingId !== null}
                        onClick={() => void handleDownload(m.id)}
                      >
                        {isDl ? (
                          <>
                            <Loader2 className="w-3.5 h-3.5 animate-spin" />
                            {pct}%
                          </>
                        ) : (
                          <>
                            <Download className="w-3.5 h-3.5" />
                            Download
                          </>
                        )}
                      </Button>
                    )}
                  </div>

                  {isDl && (
                    <div className="w-full h-1.5 bg-zinc-800 rounded-full overflow-hidden">
                      <div
                        className="h-full bg-indigo-500 transition-all duration-200"
                        style={{ width: `${Math.max(4, pct)}%` }}
                      />
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        )}

        <div className="flex items-center justify-between pt-2 border-t border-zinc-800">
          <Button variant="secondary" size="sm" onClick={() => void handleUnload()}>
            <Trash2 className="w-3.5 h-3.5 text-amber-400" />
            Unload Model from VRAM
          </Button>
          <Button size="sm" onClick={onClose}>
            Done
          </Button>
        </div>
      </Card>
    </div>
  );
}

