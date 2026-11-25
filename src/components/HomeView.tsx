import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import {
  AlertCircle,
  CheckCircle2,
  Clock,
  Cpu,
  Edit3,
  Film,
  Languages,
  Loader2,
  Plus,
  Sparkles,
  SquareX,
  Trash2,
  Upload,
  X,
} from "lucide-react";
import { toast } from "sonner";
import {
  cleanStorageJunk,
  deleteProject,
  formatTauriError,
  importVideo,
  listProjects,
  listWhisperModels,
  stopAndClearQueue,
} from "@/lib/tauri";
import { useAppStore } from "@/stores/appStore";
import { LANGUAGE_OPTIONS, type VideoProject } from "@/types/studio";
import { Button } from "@/components/ui/button";
import { Badge, Card } from "@/components/ui/card";

const VIDEO_EXTS = new Set(["mp4", "mov", "mkv", "avi", "webm", "flv", "wmv", "m4v"]);

function isVideoPath(p: string): boolean {
  const dot = p.lastIndexOf(".");
  if (dot < 0) return false;
  return VIDEO_EXTS.has(p.slice(dot + 1).toLowerCase());
}

export function HomeView() {
  const queryClient = useQueryClient();
  const { selectedLanguage, setSelectedLanguage, setRoute, setModelsModalOpen } = useAppStore();
  const [stagedPaths, setStagedPaths] = useState<string[]>([]);
  const [isDragging, setIsDragging] = useState(false);
  const [isImporting, setIsImporting] = useState(false);
  const [isCleaning, setIsCleaning] = useState(false);
  const [isClearingQueue, setIsClearingQueue] = useState(false);
  const [projectToDelete, setProjectToDelete] = useState<VideoProject | null>(null);

  const { data: projects = [], isLoading } = useQuery({
    queryKey: ["projects"],
    queryFn: listProjects,
    refetchInterval: 3000,
  });

  const { data: models = [] } = useQuery({
    queryKey: ["whisper-models"],
    queryFn: listWhisperModels,
  });

  const hasAnyModel = models.some((m) => m.downloaded);

  useEffect(() => {
    const unlisten = getCurrentWebview().onDragDropEvent((event) => {
      if (event.payload.type === "over") {
        setIsDragging(true);
      } else if (event.payload.type === "leave") {
        setIsDragging(false);
      } else if (event.payload.type === "drop") {
        setIsDragging(false);
        const picked = event.payload.paths.filter(isVideoPath);
        if (picked.length > 0) {
          setStagedPaths(picked);
        } else {
          toast.error("Drop MP4, MOV, MKV, or WEBM video files.");
        }
      }
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, []);

  const handleBrowseVideos = async () => {
    try {
      const selected = await openFileDialog({
        multiple: true,
        filters: [
          {
            name: "Video Files",
            extensions: ["mp4", "mov", "mkv", "avi", "webm", "m4v"],
          },
        ],
      });
      if (!selected) return;
      const list = Array.isArray(selected) ? selected : [selected];
      if (list.length > 0) {
        setStagedPaths(list);
      }
    } catch (err) {
      toast.error(formatTauriError(err, "Could not open file picker"));
    }
  };

  const handleConfirmImport = async () => {
    if (stagedPaths.length === 0) return;
    if (!hasAnyModel) {
      toast.warning("No Whisper model downloaded yet — open AI Models to download one.");
    }
    setIsImporting(true);
    try {
      const imported: VideoProject[] = [];
      for (const path of stagedPaths) {
        const proj = await importVideo(path, selectedLanguage, hasAnyModel);
        imported.push(proj);
      }
      setStagedPaths([]);
      await queryClient.invalidateQueries({ queryKey: ["projects"] });
      if (imported.length === 1 && imported[0]) {
        setRoute({ view: "editor", projectId: imported[0].id });
      } else {
        toast.success(`Imported ${imported.length} videos`);
      }
    } catch (err) {
      toast.error(formatTauriError(err, "Video import failed"));
    } finally {
      setIsImporting(false);
    }
  };

  const handleCleanJunk = async () => {
    setIsCleaning(true);
    try {
      const res = await cleanStorageJunk();
      toast.success(res.message);
    } catch (err) {
      toast.error(formatTauriError(err));
    } finally {
      setIsCleaning(false);
    }
  };

  const handleClearQueue = async () => {
    setIsClearingQueue(true);
    try {
      const resetCount = await stopAndClearQueue();
      toast.success(`Stopped active tasks and reset ${resetCount} project(s)`);
      await queryClient.invalidateQueries({ queryKey: ["projects"] });
    } catch (err) {
      toast.error(formatTauriError(err));
    } finally {
      setIsClearingQueue(false);
    }
  };

  const handleDeleteConfirm = async () => {
    if (!projectToDelete) return;
    try {
      await deleteProject(projectToDelete.id);
      toast.success(`Deleted ${projectToDelete.originalFilename}`);
      setProjectToDelete(null);
      await queryClient.invalidateQueries({ queryKey: ["projects"] });
    } catch (err) {
      toast.error(formatTauriError(err));
    }
  };

  return (
    <div className="space-y-8">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-2xl font-black tracking-tight text-white">
            AI Subtitles for Your Videos
          </h2>
          <p className="text-xs text-zinc-400">
            100% Offline Windows Studio &bull; Whisper Vulkan/CPU &bull; Skia WYSIWYG &bull; NVENC/AMF/QSV/x264
          </p>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          {!hasAnyModel && (
            <Button variant="default" size="sm" onClick={() => setModelsModalOpen(true)}>
              <Cpu className="w-3.5 h-3.5" />
              Download AI Model
            </Button>
          )}
          <Button
            variant="destructive"
            size="sm"
            disabled={isClearingQueue}
            onClick={() => void handleClearQueue()}
          >
            {isClearingQueue ? (
              <Loader2 className="w-3.5 h-3.5 animate-spin" />
            ) : (
              <SquareX className="w-3.5 h-3.5" />
            )}
            Stop &amp; Clear Queue
          </Button>
          <Button
            variant="secondary"
            size="sm"
            disabled={isCleaning}
            onClick={() => void handleCleanJunk()}
          >
            {isCleaning ? (
              <Loader2 className="w-3.5 h-3.5 animate-spin" />
            ) : (
              <Trash2 className="w-3.5 h-3.5" />
            )}
            Clean Storage Junk
          </Button>
        </div>
      </div>

      {/* Dropzone */}
      <div
        onClick={() => void handleBrowseVideos()}
        className={`rounded-3xl border-2 border-dashed p-12 text-center cursor-pointer transition-all ${
          isDragging
            ? "border-indigo-500 bg-indigo-500/10 scale-[1.01] shadow-2xl"
            : "border-zinc-800 bg-zinc-900/40 hover:border-indigo-500/70 hover:bg-zinc-900/70"
        }`}
      >
        <div className="flex flex-col items-center space-y-4">
          <div className="w-20 h-20 rounded-3xl bg-indigo-500/15 border border-indigo-500/30 flex items-center justify-center text-indigo-400">
            <Upload className="w-10 h-10" />
          </div>
          <div className="space-y-1.5">
            <h3 className="text-xl font-extrabold text-white flex items-center justify-center gap-2">
              Drag &amp; Drop Video(s) Here
              <Sparkles className="w-5 h-5 text-amber-400" />
            </h3>
            <p className="text-xs text-zinc-400">
              Select one or multiple MP4, MOV, MKV, or WEBM clips directly from disk (no duplicate copy required)
            </p>
          </div>
          <Button size="lg">
            <Plus className="w-4 h-4" />
            Select Video File(s)
          </Button>
        </div>
      </div>

      {/* Project List */}
      <div className="space-y-4">
        <div className="flex items-center justify-between">
          <h3 className="text-sm font-bold text-zinc-200 flex items-center gap-2">
            <Film className="w-4 h-4 text-indigo-400" />
            Projects ({projects.length})
          </h3>
        </div>

        {isLoading ? (
          <div className="py-10 flex items-center justify-center text-xs text-zinc-400 gap-2">
            <Loader2 className="w-4 h-4 animate-spin text-indigo-400" />
            Loading projects...
          </div>
        ) : projects.length === 0 ? (
          <Card className="p-8 text-center text-xs text-zinc-400">
            No video projects yet. Drop or select a video above to start transcribing offline.
          </Card>
        ) : (
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            {projects.map((p) => {
              const sec = Math.round(p.durationMs / 1000);
              const mins = Math.floor(sec / 60);
              const rem = sec % 60;
              const formatted = `${mins}:${rem.toString().padStart(2, "0")}`;
              return (
                <Card key={p.id} className="p-5 space-y-4 hover:border-indigo-500/50 transition-colors">
                  <div className="flex items-start justify-between gap-3">
                    <div className="min-w-0 space-y-1">
                      <h4 className="text-sm font-bold text-white truncate" title={p.originalFilename}>
                        {p.originalFilename}
                      </h4>
                      <div className="flex items-center gap-3 text-[11px] text-zinc-400">
                        <span className="flex items-center gap-1">
                          <Clock className="w-3 h-3" />
                          {formatted}
                        </span>
                        <span>
                          {p.width}x{p.height} &bull; {p.fps} fps &bull; {p.cueCount} cues
                        </span>
                      </div>
                    </div>

                    {p.status === "TRANSCRIBED" ? (
                      <Badge variant="success">
                        <CheckCircle2 className="w-3 h-3" /> Ready
                      </Badge>
                    ) : p.status === "FAILED" ? (
                      <Badge variant="destructive">
                        <AlertCircle className="w-3 h-3" /> Failed
                      </Badge>
                    ) : (
                      <Badge variant="default">
                        <Loader2 className="w-3 h-3 animate-spin" /> {p.status}
                      </Badge>
                    )}
                  </div>

                  <div className="flex items-center gap-2">
                    <Button
                      className="flex-1"
                      variant="secondary"
                      onClick={() => setRoute({ view: "editor", projectId: p.id })}
                    >
                      <Edit3 className="w-3.5 h-3.5 text-indigo-400" />
                      Open Studio Editor
                    </Button>
                    <Button
                      variant="destructive"
                      size="icon"
                      onClick={() => setProjectToDelete(p)}
                      title="Delete Project"
                    >
                      <Trash2 className="w-4 h-4" />
                    </Button>
                  </div>
                </Card>
              );
            })}
          </div>
        )}
      </div>

      {/* Language Selection Modal before Import */}
      {stagedPaths.length > 0 && (
        <div className="fixed inset-0 z-50 bg-black/80 backdrop-blur-md flex items-center justify-center p-4">
          <Card className="max-w-lg w-full p-6 space-y-5 bg-zinc-950 border-zinc-800 relative">
            <button
              type="button"
              onClick={() => setStagedPaths([])}
              className="absolute top-4 right-4 p-1.5 rounded-lg text-zinc-400 hover:text-white hover:bg-zinc-800 cursor-pointer"
            >
              <X className="w-4 h-4" />
            </button>

            <div className="flex items-center gap-3">
              <div className="p-2.5 rounded-xl bg-indigo-500/15 text-indigo-400">
                <Languages className="w-5 h-5" />
              </div>
              <div>
                <h3 className="text-base font-bold text-white">Select Target Speech Language</h3>
                <p className="text-xs text-zinc-400">
                  {stagedPaths.length} video file(s) staged for offline transcription
                </p>
              </div>
            </div>

            <div className="max-h-72 overflow-y-auto space-y-1.5 pr-1">
              {LANGUAGE_OPTIONS.map((lang) => {
                const active = selectedLanguage === lang.code;
                return (
                  <button
                    key={lang.code}
                    type="button"
                    onClick={() => setSelectedLanguage(lang.code)}
                    className={`w-full p-2.5 rounded-xl border text-left transition-all cursor-pointer ${
                      active
                        ? "border-indigo-500 bg-indigo-500/15 text-white"
                        : "border-zinc-800 bg-zinc-900/50 text-zinc-300 hover:border-zinc-700"
                    }`}
                  >
                    <div className="text-xs font-bold">{lang.name}</div>
                    <div className="text-[10px] text-zinc-400">{lang.desc}</div>
                  </button>
                );
              })}
            </div>

            <div className="flex items-center justify-end gap-2 pt-2 border-t border-zinc-800">
              <Button variant="secondary" onClick={() => setStagedPaths([])} disabled={isImporting}>
                Cancel
              </Button>
              <Button onClick={() => void handleConfirmImport()} disabled={isImporting}>
                {isImporting ? (
                  <Loader2 className="w-3.5 h-3.5 animate-spin" />
                ) : (
                  <Sparkles className="w-3.5 h-3.5 text-amber-300" />
                )}
                Start AI Transcription
              </Button>
            </div>
          </Card>
        </div>
      )}

      {/* Delete Confirmation Modal */}
      {projectToDelete && (
        <div className="fixed inset-0 z-50 bg-black/80 backdrop-blur-md flex items-center justify-center p-4">
          <Card className="max-w-md w-full p-6 space-y-4 bg-zinc-950 border-red-500/30">
            <h4 className="text-base font-bold text-white">Delete Subtitle Project?</h4>
            <p className="text-xs text-zinc-400">
              Permanently remove <strong className="text-white">{projectToDelete.originalFilename}</strong> and its database cues/exports? Your original source video file on disk will not be deleted.
            </p>
            <div className="flex justify-end gap-2">
              <Button variant="secondary" onClick={() => setProjectToDelete(null)}>
                Cancel
              </Button>
              <Button variant="destructive" onClick={() => void handleDeleteConfirm()}>
                Delete Project
              </Button>
            </div>
          </Card>
        </div>
      )}
    </div>
  );
}

