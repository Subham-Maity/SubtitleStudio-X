import { useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { save as saveFileDialog } from "@tauri-apps/plugin-dialog";
import { motion } from "motion/react";
import {
  AlertTriangle,
  CheckCircle2,
  Cpu,
  Film,
  FolderOpen,
  Layers,
  Loader2,
  Square,
  Video,
  Volume2,
  VolumeX,
  X,
} from "lucide-react";
import { toast } from "sonner";
import {
  cancelExport,
  detectExportEncoder,
  formatTauriError,
  revealInExplorer,
  startExport,
} from "@/lib/tauri";
import type {
  ExportFormat,
  ExportMode,
  ExportProgressEvent,
  ExportQuality,
  VideoProject,
} from "@/types/studio";
import { Button } from "@/components/ui/button";
import { Badge, Card } from "@/components/ui/card";

interface ExportModalProps {
  open: boolean;
  onClose: () => void;
  project: VideoProject;
}

export function ExportModal({ open, onClose, project }: ExportModalProps) {
  const [mode, setMode] = useState<ExportMode>("OVERLAY");
  const [format, setFormat] = useState<ExportFormat>("MP4_H265");
  const [quality, setQuality] = useState<ExportQuality>("balanced");
  const [backgroundHex, setBackgroundHex] = useState("#000000");
  const [includeAudio, setIncludeAudio] = useState(true);

  const [activeJobId, setActiveJobId] = useState<string | null>(null);
  const [isExporting, setIsExporting] = useState(false);
  const [progressPct, setProgressPct] = useState(0);
  const [outputPath, setOutputPath] = useState<string | null>(null);
  const [encoderLabel, setEncoderLabel] = useState<string | null>(null);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  const { data: detectedEncoder, isLoading: detectingEncoder } = useQuery({
    queryKey: ["encoder-probe", format],
    queryFn: () => detectExportEncoder(format),
    enabled: open,
  });

  useEffect(() => {
    if (!open) return;
    const unlisten = listen<ExportProgressEvent>("export://progress", (ev) => {
      const p = ev.payload;
      if (p.projectId !== project.id) return;
      if (activeJobId && p.jobId !== activeJobId) return;

      if (p.encoder) setEncoderLabel(p.encoder);
      if (p.fallbackNotice) {
        toast.warning(p.fallbackNotice);
      }

      if (p.eventType === "progress") {
        setProgressPct(p.progressPct);
      } else if (p.eventType === "done") {
        setProgressPct(100);
        setIsExporting(false);
        setOutputPath(p.outputPath);
        toast.success("Video export complete!");
      } else if (p.eventType === "error") {
        setIsExporting(false);
        setErrorMsg(p.message ?? "Export failed");
      }
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [open, project.id, activeJobId]);

  if (!open) return null;

  const handleStart = async () => {
    const ext = format === "MOV" ? "mov" : "mp4";
    const baseName = project.originalFilename.replace(/\.[^.]+$/, "");
    let destPath: string | undefined;
    try {
      const picked = await saveFileDialog({
        defaultPath: `${baseName}-subtitled.${ext}`,
        filters: [{ name: format === "MOV" ? "QuickTime MOV" : "MP4 Video", extensions: [ext] }],
      });
      if (!picked) return;
      destPath = picked;
    } catch (err) {
      toast.error(formatTauriError(err, "Save dialog failed"));
      return;
    }

    setIsExporting(true);
    setProgressPct(1);
    setOutputPath(null);
    setErrorMsg(null);

    try {
      const job = await startExport({
        projectId: project.id,
        mode,
        format,
        quality,
        backgroundHex,
        includeAudio,
        destPath,
      });
      setActiveJobId(job.id);
    } catch (err) {
      setIsExporting(false);
      setErrorMsg(formatTauriError(err, "Could not start export"));
    }
  };

  const handleCancel = async () => {
    if (!activeJobId) return;
    try {
      await cancelExport(activeJobId);
      setIsExporting(false);
      setProgressPct(0);
      toast.info("Export cancelled and partial file cleaned up");
    } catch (err) {
      toast.error(formatTauriError(err));
    }
  };

  return (
    <motion.div
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      transition={{ duration: 0.16 }}
      className="fixed inset-0 z-50 bg-black/80 backdrop-blur-md flex items-center justify-center p-4"
    >
      <motion.div
        initial={{ opacity: 0, scale: 0.96, y: 8 }}
        animate={{ opacity: 1, scale: 1, y: 0 }}
        transition={{ duration: 0.2, ease: [0.16, 1, 0.3, 1] }}
        className="max-w-xl w-full"
      >
        <Card className="w-full p-6 space-y-5 bg-zinc-950/95 border-zinc-800 relative">
        <button
          type="button"
          onClick={onClose}
          className="absolute top-4 right-4 p-1.5 rounded-lg text-zinc-400 hover:text-white hover:bg-zinc-800 cursor-pointer"
        >
          <X className="w-4 h-4" />
        </button>

        <div className="flex items-center justify-between pr-8">
          <div className="flex items-center gap-2.5">
            <Film className="w-5 h-5 text-indigo-400" />
            <h3 className="text-base font-bold text-white">Native GPU Video Export</h3>
          </div>
          {detectingEncoder ? (
            <Badge variant="default">Probing GPU...</Badge>
          ) : detectedEncoder ? (
            <Badge variant={detectedEncoder.isHardware ? "success" : "warning"}>
              <Cpu className="w-3 h-3" />
              {encoderLabel ?? detectedEncoder.displayName}
            </Badge>
          ) : null}
        </div>

        {detectedEncoder?.fallbackNotice && (
          <div className="p-3 rounded-xl bg-amber-500/10 border border-amber-500/30 text-amber-300 text-xs flex items-start gap-2">
            <AlertTriangle className="w-4 h-4 shrink-0 mt-0.5" />
            <span>{detectedEncoder.fallbackNotice}</span>
          </div>
        )}

        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          <div className="space-y-1.5">
            <label className="text-xs font-medium text-zinc-300 flex items-center gap-1.5">
              <Layers className="w-3.5 h-3.5 text-indigo-400" /> Export Mode
            </label>
            <div className="grid grid-cols-2 gap-2">
              <button
                type="button"
                onClick={() => setMode("OVERLAY")}
                disabled={isExporting}
                className={`p-2.5 rounded-xl border text-left text-xs transition-all cursor-pointer ${
                  mode === "OVERLAY"
                    ? "bg-indigo-600/20 border-indigo-500 text-white"
                    : "bg-zinc-900 border-zinc-800 text-zinc-400"
                }`}
              >
                <div className="font-semibold flex items-center gap-1">
                  <Video className="w-3.5 h-3.5" /> Overlay
                </div>
                <div className="text-[10px] text-zinc-400">Burned on video</div>
              </button>
              <button
                type="button"
                onClick={() => setMode("CAPTIONS_ONLY")}
                disabled={isExporting}
                className={`p-2.5 rounded-xl border text-left text-xs transition-all cursor-pointer ${
                  mode === "CAPTIONS_ONLY"
                    ? "bg-indigo-600/20 border-indigo-500 text-white"
                    : "bg-zinc-900 border-zinc-800 text-zinc-400"
                }`}
              >
                <div className="font-semibold flex items-center gap-1">
                  <Film className="w-3.5 h-3.5" /> Captions-Only
                </div>
                <div className="text-[10px] text-zinc-400">Solid BG + Audio</div>
              </button>
            </div>
          </div>

          <div className="space-y-1.5">
            <label className="text-xs font-medium text-zinc-300">Container &amp; Codec</label>
            <div className="grid grid-cols-2 gap-2">
              <button
                type="button"
                onClick={() => setFormat("MP4_H265")}
                disabled={isExporting}
                className={`p-2.5 rounded-xl border text-xs font-semibold cursor-pointer ${
                  format === "MP4_H265"
                    ? "bg-indigo-600 border-indigo-500 text-white"
                    : "bg-zinc-900 border-zinc-800 text-zinc-400"
                }`}
              >
                MP4 (H.264)
              </button>
              <button
                type="button"
                onClick={() => setFormat("MOV")}
                disabled={isExporting}
                className={`p-2.5 rounded-xl border text-xs font-semibold cursor-pointer ${
                  format === "MOV"
                    ? "bg-indigo-600 border-indigo-500 text-white"
                    : "bg-zinc-900 border-zinc-800 text-zinc-400"
                }`}
              >
                MOV (HEVC)
              </button>
            </div>
          </div>
        </div>

        {/* Quality Preset */}
        <div className="space-y-1.5">
          <label className="text-xs font-medium text-zinc-300">Quality Preset</label>
          <div className="grid grid-cols-3 gap-2">
            {(["fast", "balanced", "quality"] as const).map((q) => (
              <button
                key={q}
                type="button"
                onClick={() => setQuality(q)}
                disabled={isExporting}
                className={`py-2 rounded-xl border text-xs font-semibold capitalize cursor-pointer ${
                  quality === q
                    ? "bg-indigo-600 border-indigo-500 text-white"
                    : "bg-zinc-900 border-zinc-800 text-zinc-400"
                }`}
              >
                {q}
              </button>
            ))}
          </div>
        </div>

        {mode === "CAPTIONS_ONLY" && (
          <div className="p-3.5 rounded-xl bg-zinc-900/70 border border-zinc-800 space-y-3">
            <div className="flex items-center justify-between">
              <span className="text-xs text-zinc-300">Background Color</span>
              <div className="flex items-center gap-2">
                {["#000000", "#FFFFFF", "#00FF00"].map((hex) => (
                  <button
                    key={hex}
                    type="button"
                    onClick={() => setBackgroundHex(hex)}
                    className={`w-6 h-6 rounded-full border-2 cursor-pointer ${
                      backgroundHex === hex ? "border-indigo-500 scale-110" : "border-zinc-700"
                    }`}
                    style={{ backgroundColor: hex }}
                  />
                ))}
                <input
                  type="color"
                  value={backgroundHex}
                  onChange={(e) => setBackgroundHex(e.target.value)}
                  className="w-7 h-7 bg-transparent cursor-pointer"
                />
              </div>
            </div>
            <div className="flex items-center justify-between pt-2 border-t border-zinc-800">
              <span className="text-xs text-zinc-300 flex items-center gap-1.5">
                {includeAudio ? <Volume2 className="w-3.5 h-3.5 text-indigo-400" /> : <VolumeX className="w-3.5 h-3.5 text-zinc-500" />}
                Include Original Audio
              </span>
              <input
                type="checkbox"
                checked={includeAudio}
                onChange={(e) => setIncludeAudio(e.target.checked)}
                className="w-4 h-4 accent-indigo-500 cursor-pointer"
              />
            </div>
          </div>
        )}

        {isExporting && (
          <div className="p-4 rounded-xl bg-zinc-900 border border-indigo-500/30 space-y-3">
            <div className="flex items-center justify-between text-xs">
              <span className="flex items-center gap-2 font-semibold text-white">
                <Loader2 className="w-4 h-4 animate-spin text-indigo-400" />
                Rendering RGBA frames &amp; encoding ({encoderLabel ?? detectedEncoder?.displayName ?? "FFmpeg"})...
              </span>
              <span className="font-mono font-bold text-indigo-400">{progressPct}%</span>
            </div>
            <div className="w-full h-2.5 bg-zinc-800 rounded-full overflow-hidden">
              <div
                className="h-full bg-gradient-to-r from-indigo-500 to-cyan-400 transition-all duration-200"
                style={{ width: `${progressPct}%` }}
              />
            </div>
            <Button variant="destructive" className="w-full" onClick={() => void handleCancel()}>
              <Square className="w-3.5 h-3.5" />
              Cancel Export
            </Button>
          </div>
        )}

        {outputPath && !isExporting && (
          <div className="space-y-2.5">
            <div className="p-3 rounded-xl bg-emerald-500/10 border border-emerald-500/30 text-emerald-300 text-xs flex items-center gap-2">
              <CheckCircle2 className="w-4 h-4 shrink-0" />
              <span className="truncate">{outputPath}</span>
            </div>
            <div className="flex gap-2">
              <Button className="flex-1" onClick={() => void revealInExplorer(outputPath)}>
                <FolderOpen className="w-4 h-4" />
                Show in Explorer
              </Button>
              <Button variant="secondary" onClick={() => setOutputPath(null)}>
                Export Again
              </Button>
            </div>
          </div>
        )}

        {errorMsg && (
          <div className="p-3 rounded-xl bg-red-500/10 border border-red-500/30 text-red-400 text-xs">
            {errorMsg}
          </div>
        )}

        {!isExporting && !outputPath && (
          <Button className="w-full" size="lg" onClick={() => void handleStart()}>
            <Film className="w-4 h-4" />
            Render &amp; Export Video (100% WYSIWYG)
          </Button>
        )}
      </Card>
      </motion.div>
    </motion.div>
  );
}

