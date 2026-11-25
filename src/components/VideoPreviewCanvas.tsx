import { useEffect, useRef, useState, type RefObject } from "react";
import { Check, RotateCw } from "lucide-react";
import { toast } from "sonner";
import {
  formatTauriError,
  localFileAssetUrl,
  renderPreviewFrame,
  updateCue,
  updateStyle,
} from "@/lib/tauri";
import type { SubtitleCue, SubtitleStyle, VideoProject } from "@/types/studio";

interface VideoPreviewCanvasProps {
  project: VideoProject;
  videoRef: RefObject<HTMLVideoElement | null>;
  currentTimeMs: number;
  onTimeUpdate: (ms: number) => void;
  onProjectUpdated: () => void;
  activeCueOverride: SubtitleCue | null;
}

export function VideoPreviewCanvas({
  project,
  videoRef,
  currentTimeMs,
  onTimeUpdate,
  onProjectUpdated,
  activeCueOverride,
}: VideoPreviewCanvasProps) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const handleBoxRef = useRef<HTMLDivElement | null>(null);

  const style: SubtitleStyle | null = project.style ?? null;
  const activePlaybackCue =
    project.cues.find((c) => currentTimeMs >= c.startMs && currentTimeMs <= c.endMs) ?? null;
  const currentCue = activeCueOverride ?? activePlaybackCue ?? project.cues[0] ?? null;

  const [isSelected, setIsSelected] = useState(true);
  const [isEditingInline, setIsEditingInline] = useState(false);
  const [inlineText, setInlineText] = useState("");

  const [posX, setPosX] = useState(style?.positionX ?? 50);
  const [posY, setPosY] = useState(style?.positionY ?? 80);
  const [fontSize, setFontSize] = useState(style?.fontSizePx ?? 42);
  const [rotation, setRotation] = useState(style?.rotationDeg ?? 0);

  const posRef = useRef({ x: style?.positionX ?? 50, y: style?.positionY ?? 80 });
  const sizeRef = useRef(style?.fontSizePx ?? 42);
  const rotRef = useRef(style?.rotationDeg ?? 0);

  const isDraggingRef = useRef(false);
  const isResizingRef = useRef(false);
  const isRotatingRef = useRef(false);
  const dragStartRef = useRef({ clientX: 0, clientY: 0, initX: 50, initY: 80 });
  const resizeStartRef = useRef({ clientY: 0, initSize: 42 });
  const rotateCenterRef = useRef({ cx: 0, cy: 0 });

  useEffect(() => {
    if (style && !isDraggingRef.current && !isResizingRef.current && !isRotatingRef.current) {
      setPosX(style.positionX);
      setPosY(style.positionY);
      setFontSize(style.fontSizePx);
      setRotation(style.rotationDeg);
      posRef.current = { x: style.positionX, y: style.positionY };
      sizeRef.current = style.fontSizePx;
      rotRef.current = style.rotationDeg;
    }
  }, [style]);

  // Render subtitle frame via the exact Rust `subtitle_render::render_frame` backend command.
  useEffect(() => {
    let cancelled = false;
    let inFlight = false;
    let pendingTimeMs: number | null = null;

    const drawAtTime = async (tMs: number) => {
      if (cancelled) return;
      if (inFlight) {
        pendingTimeMs = tMs;
        return;
      }
      const canvas = canvasRef.current;
      const container = containerRef.current;
      if (!canvas || !container) return;

      const rect = container.getBoundingClientRect();
      const targetW = Math.max(2, Math.round(rect.width));
      const targetH = Math.max(2, Math.round(rect.height));
      if (canvas.width !== targetW || canvas.height !== targetH) {
        canvas.width = targetW;
        canvas.height = targetH;
      }

      inFlight = true;
      try {
        const rgba = await renderPreviewFrame(project.id, tMs, targetW, targetH);
        if (cancelled) return;
        const ctx = canvas.getContext("2d");
        if (ctx && rgba.length === targetW * targetH * 4) {
          const img = new ImageData(new Uint8ClampedArray(rgba), targetW, targetH);
          ctx.putImageData(img, 0, 0);
        }
      } catch {
        // Ignore transient render skips during scrub.
      } finally {
        inFlight = false;
        if (pendingTimeMs !== null && !cancelled) {
          const next = pendingTimeMs;
          pendingTimeMs = null;
          void drawAtTime(next);
        }
      }
    };

    void drawAtTime(activeCueOverride ? activeCueOverride.startMs : currentTimeMs);

    const videoEl = videoRef.current;
    let rvfcId: number | null = null;
    const onVideoFrame = (_now: number, meta: VideoFrameCallbackMetadata) => {
      if (cancelled) return;
      const ms = Math.round(meta.mediaTime * 1000);
      onTimeUpdate(ms);
      void drawAtTime(ms);
      if (videoEl && typeof videoEl.requestVideoFrameCallback === "function") {
        rvfcId = videoEl.requestVideoFrameCallback(onVideoFrame);
      }
    };

    if (videoEl && typeof videoEl.requestVideoFrameCallback === "function") {
      rvfcId = videoEl.requestVideoFrameCallback(onVideoFrame);
    }

    return () => {
      cancelled = true;
      if (rvfcId !== null && videoEl && typeof videoEl.cancelVideoFrameCallback === "function") {
        videoEl.cancelVideoFrameCallback(rvfcId);
      }
    };
  }, [
    project.id,
    project.cues,
    project.style,
    currentTimeMs,
    activeCueOverride,
    videoRef,
    onTimeUpdate,
  ]);

  const commitStylePatch = async (patch: Parameters<typeof updateStyle>[1]) => {
    try {
      await updateStyle(project.id, patch);
      onProjectUpdated();
    } catch (err) {
      toast.error(formatTauriError(err, "Failed to update subtitle layout"));
    }
  };

  // Drag position handlers
  const onMouseDownDrag = (e: React.MouseEvent) => {
    e.stopPropagation();
    setIsSelected(true);
    isDraggingRef.current = true;
    dragStartRef.current = {
      clientX: e.clientX,
      clientY: e.clientY,
      initX: posRef.current.x,
      initY: posRef.current.y,
    };
    const onMove = (ev: MouseEvent) => {
      if (!isDraggingRef.current || !containerRef.current) return;
      const rect = containerRef.current.getBoundingClientRect();
      const dxPct = ((ev.clientX - dragStartRef.current.clientX) / rect.width) * 100;
      const dyPct = ((ev.clientY - dragStartRef.current.clientY) / rect.height) * 100;
      const nx = Math.round(Math.min(95, Math.max(5, dragStartRef.current.initX + dxPct)) * 10) / 10;
      const ny = Math.round(Math.min(95, Math.max(5, dragStartRef.current.initY + dyPct)) * 10) / 10;
      posRef.current = { x: nx, y: ny };
      setPosX(nx);
      setPosY(ny);
    };
    const onUp = () => {
      if (isDraggingRef.current) {
        isDraggingRef.current = false;
        void commitStylePatch({ positionX: posRef.current.x, positionY: posRef.current.y });
      }
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  };

  // Corner resize handler
  const onMouseDownResize = (e: React.MouseEvent) => {
    e.stopPropagation();
    isResizingRef.current = true;
    resizeStartRef.current = {
      clientY: e.clientY,
      initSize: sizeRef.current,
    };
    const onMove = (ev: MouseEvent) => {
      if (!isResizingRef.current) return;
      const dy = resizeStartRef.current.clientY - ev.clientY;
      const next = Math.min(200, Math.max(10, Math.round(resizeStartRef.current.initSize + dy * 0.5)));
      sizeRef.current = next;
      setFontSize(next);
    };
    const onUp = () => {
      if (isResizingRef.current) {
        isResizingRef.current = false;
        void commitStylePatch({ fontSizePx: sizeRef.current });
      }
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  };

  // Rotation handle
  const onMouseDownRotate = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (!handleBoxRef.current) return;
    isRotatingRef.current = true;
    const rect = handleBoxRef.current.getBoundingClientRect();
    rotateCenterRef.current = {
      cx: rect.left + rect.width / 2,
      cy: rect.top + rect.height / 2,
    };
    const onMove = (ev: MouseEvent) => {
      if (!isRotatingRef.current) return;
      const dx = ev.clientX - rotateCenterRef.current.cx;
      const dy = ev.clientY - rotateCenterRef.current.cy;
      let deg = Math.round(Math.atan2(dy, dx) * (180 / Math.PI)) + 90;
      if (deg < 0) deg += 360;
      rotRef.current = deg;
      setRotation(deg);
    };
    const onUp = () => {
      if (isRotatingRef.current) {
        isRotatingRef.current = false;
        void commitStylePatch({ rotationDeg: rotRef.current });
      }
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  };

  const handleSaveInline = async () => {
    if (!currentCue) return;
    try {
      await updateCue(currentCue.id, { text: inlineText });
      setIsEditingInline(false);
      onProjectUpdated();
    } catch (err) {
      toast.error(formatTauriError(err, "Failed to update cue text"));
    }
  };

  const videoSrc = localFileAssetUrl(project.storagePath);
  const aspect = project.width > 0 && project.height > 0 ? `${project.width} / ${project.height}` : "16 / 9";

  return (
    <div
      ref={containerRef}
      onClick={() => setIsSelected(false)}
      className="relative w-full rounded-2xl overflow-hidden border border-zinc-800 bg-black shadow-2xl mx-auto flex items-center justify-center"
      style={{ aspectRatio: aspect, maxHeight: "58vh" }}
    >
      <video
        ref={videoRef}
        src={videoSrc}
        controls
        onTimeUpdate={() => {
          if (videoRef.current) {
            onTimeUpdate(Math.round(videoRef.current.currentTime * 1000));
          }
        }}
        className="w-full h-full object-contain"
      />

      {/* Exact Rust Skia RGBA Overlay Canvas */}
      <canvas
        ref={canvasRef}
        className="absolute inset-0 w-full h-full pointer-events-none z-20"
      />

      {/* Interactive DOM Manipulation Handles (Drag, 4-Corner Resize, Rotate, Inline Edit) */}
      {currentCue && style && (
        <div
          ref={handleBoxRef}
          onMouseDown={onMouseDownDrag}
          onDoubleClick={(e) => {
            e.stopPropagation();
            setInlineText(currentCue.text);
            setIsEditingInline(true);
          }}
          className="absolute z-30 cursor-move select-none"
          style={{
            left: `${posX}%`,
            top: `${posY}%`,
            transform: `translate(-50%, -50%) rotate(${rotation}deg)`,
            minWidth: `${Math.max(120, fontSize * 3)}px`,
            minHeight: `${Math.max(40, fontSize * 1.4)}px`,
            touchAction: "none",
          }}
        >
          <div
            className={`relative w-full h-full rounded-xl transition-all ${
              isSelected
                ? "ring-2 ring-cyan-400/80 bg-cyan-500/5"
                : "hover:ring-1 hover:ring-cyan-400/40"
            }`}
          >
            {isSelected && (
              <>
                <div
                  onMouseDown={onMouseDownRotate}
                  title={`Rotate (${rotation}°)`}
                  className="absolute -top-9 left-1/2 -translate-x-1/2 w-6 h-6 rounded-full bg-zinc-900 border-2 border-cyan-400 flex items-center justify-center cursor-grab shadow-lg"
                >
                  <RotateCw className="w-3 h-3 text-cyan-300" />
                </div>
                <div
                  onMouseDown={onMouseDownResize}
                  className="absolute -top-1.5 -left-1.5 w-3.5 h-3.5 rounded-full bg-white border-2 border-cyan-500 cursor-nwse-resize"
                />
                <div
                  onMouseDown={onMouseDownResize}
                  className="absolute -top-1.5 -right-1.5 w-3.5 h-3.5 rounded-full bg-white border-2 border-cyan-500 cursor-nesw-resize"
                />
                <div
                  onMouseDown={onMouseDownResize}
                  className="absolute -bottom-1.5 -left-1.5 w-3.5 h-3.5 rounded-full bg-white border-2 border-cyan-500 cursor-nesw-resize"
                />
                <div
                  onMouseDown={onMouseDownResize}
                  className="absolute -bottom-1.5 -right-1.5 w-3.5 h-3.5 rounded-full bg-white border-2 border-cyan-500 cursor-nwse-resize"
                />
              </>
            )}

            {isEditingInline && (
              <div
                className="flex items-center gap-1.5 bg-zinc-950/95 p-1.5 rounded-xl border border-cyan-400 shadow-2xl"
                onClick={(e) => e.stopPropagation()}
              >
                <input
                  type="text"
                  value={inlineText}
                  onChange={(e) => setInlineText(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") void handleSaveInline();
                    if (e.key === "Escape") setIsEditingInline(false);
                  }}
                  autoFocus
                  className="bg-transparent text-white text-sm px-2 py-1 outline-none min-w-[200px]"
                />
                <button
                  type="button"
                  onClick={() => void handleSaveInline()}
                  className="p-1.5 rounded-lg bg-cyan-500 text-black hover:bg-cyan-400 cursor-pointer"
                >
                  <Check className="w-3.5 h-3.5" />
                </button>
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
