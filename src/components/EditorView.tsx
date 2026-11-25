import { useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  AlertCircle,
  ArrowLeft,
  CheckCircle2,
  Download,
  Loader2,
  RefreshCw,
  Subtitles,
  Trash2,
} from "lucide-react";
import { toast } from "sonner";
import {
  deleteProject,
  formatTauriError,
  getProject,
  retryPipeline,
} from "@/lib/tauri";
import { useAppStore } from "@/stores/appStore";
import { Button } from "@/components/ui/button";
import { Badge, Card } from "@/components/ui/card";
import { VideoPreviewCanvas } from "@/components/VideoPreviewCanvas";
import { CanvasFloatingToolbar } from "@/components/CanvasFloatingToolbar";
import { PipelineProgressBanner } from "@/components/PipelineProgressBanner";
import { TranscriptEditorPanel } from "@/components/TranscriptEditorPanel";
import { ExportModal } from "@/components/ExportModal";

interface EditorViewProps {
  projectId: string;
}

export function EditorView({ projectId }: EditorViewProps) {
  const queryClient = useQueryClient();
  const { setRoute } = useAppStore();
  const videoRef = useRef<HTMLVideoElement | null>(null);
  const [currentTimeMs, setCurrentTimeMs] = useState(0);
  const [showExportModal, setShowExportModal] = useState(false);
  const [showDeleteModal, setShowDeleteModal] = useState(false);

  const { data: project, isLoading } = useQuery({
    queryKey: ["project", projectId],
    queryFn: () => getProject(projectId),
  });

  const refreshProject = () => {
    void queryClient.invalidateQueries({ queryKey: ["project", projectId] });
  };

  if (isLoading) {
    return (
      <div className="py-20 flex items-center justify-center gap-2 text-xs text-zinc-400">
        <Loader2 className="w-5 h-5 animate-spin text-indigo-400" />
        Loading Subtitle Studio...
      </div>
    );
  }

  if (!project) {
    return (
      <Card className="p-8 text-center space-y-3">
        <p className="text-sm text-zinc-400">Project not found.</p>
        <Button onClick={() => setRoute({ view: "home" })}>Back to Projects</Button>
      </Card>
    );
  }

  const activePlaybackCue =
    project.cues.find((c) => currentTimeMs >= c.startMs && currentTimeMs <= c.endMs) ??
    project.cues[0] ??
    null;

  const handleSeek = (ms: number) => {
    if (videoRef.current) {
      videoRef.current.currentTime = ms / 1000;
      setCurrentTimeMs(ms);
    }
  };

  const handleDelete = async () => {
    try {
      await deleteProject(project.id);
      toast.success("Project deleted");
      setRoute({ view: "home" });
    } catch (err) {
      toast.error(formatTauriError(err));
    }
  };

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3 pb-3 border-b border-zinc-800">
        <div className="flex items-center gap-3">
          <Button variant="secondary" size="icon" onClick={() => setRoute({ view: "home" })}>
            <ArrowLeft className="w-4 h-4" />
          </Button>
          <div>
            <h2 className="text-sm font-bold text-white flex items-center gap-2">
              <Subtitles className="w-4 h-4 text-indigo-400" />
              {project.originalFilename}
            </h2>
            <p className="text-[11px] text-zinc-400">
              {project.width}x{project.height} &bull; {project.fps} FPS &bull;{" "}
              {(project.durationMs / 1000).toFixed(1)}s
            </p>
          </div>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          {project.status === "TRANSCRIBED" ? (
            <Badge variant="success">
              <CheckCircle2 className="w-3 h-3" /> Transcribed
            </Badge>
          ) : project.status === "FAILED" ? (
            <Badge variant="destructive">
              <AlertCircle className="w-3 h-3" /> Failed
            </Badge>
          ) : (
            <Badge variant="default">
              <Loader2 className="w-3 h-3 animate-spin" /> {project.status}
            </Badge>
          )}

          {project.status === "FAILED" && (
            <Button
              size="sm"
              variant="secondary"
              onClick={() => void retryPipeline(project.id).then(refreshProject)}
            >
              <RefreshCw className="w-3.5 h-3.5" /> Retry
            </Button>
          )}

          <Button size="sm" onClick={() => setShowExportModal(true)}>
            <Download className="w-3.5 h-3.5" /> Export Video
          </Button>
          <Button size="sm" variant="destructive" onClick={() => setShowDeleteModal(true)}>
            <Trash2 className="w-3.5 h-3.5" /> Remove
          </Button>
        </div>
      </div>

      <VideoPreviewCanvas
        project={project}
        videoRef={videoRef}
        currentTimeMs={currentTimeMs}
        onTimeUpdate={setCurrentTimeMs}
        onProjectUpdated={refreshProject}
        activeCueOverride={null}
      />

      <CanvasFloatingToolbar
        projectId={project.id}
        currentCue={activePlaybackCue}
        style={project.style}
        onUpdated={refreshProject}
      />

      <PipelineProgressBanner
        projectId={project.id}
        status={project.status}
        errorMessage={project.errorMessage}
        onCompleted={refreshProject}
      />

      <TranscriptEditorPanel
        projectId={project.id}
        cues={project.cues}
        currentTimeMs={currentTimeMs}
        wordsPerCue={project.style?.wordsPerCue ?? 6}
        onSeek={handleSeek}
        onUpdated={refreshProject}
      />

      <ExportModal
        open={showExportModal}
        onClose={() => setShowExportModal(false)}
        project={project}
      />

      {showDeleteModal && (
        <div className="fixed inset-0 z-50 bg-black/80 backdrop-blur-sm flex items-center justify-center p-4">
          <Card className="max-w-md w-full p-6 space-y-4 bg-zinc-950 border-red-500/30">
            <h4 className="text-base font-bold text-white">Delete Video Project?</h4>
            <p className="text-xs text-zinc-400">
              Permanently remove {project.originalFilename} cues, styles, and exported clips from the studio database?
            </p>
            <div className="flex justify-end gap-2">
              <Button variant="secondary" onClick={() => setShowDeleteModal(false)}>
                Cancel
              </Button>
              <Button variant="destructive" onClick={() => void handleDelete()}>
                Delete Project
              </Button>
            </div>
          </Card>
        </div>
      )}
    </div>
  );
}

