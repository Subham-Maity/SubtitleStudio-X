import { useMemo, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Ban,
  Bold,
  Check,
  Flame,
  Italic,
  MoreHorizontal,
  Plus,
  RotateCcw,
  Sparkles,
  Square,
  Type,
  Zap,
} from "lucide-react";
import { toast } from "sonner";
import {
  formatTauriError,
  listFonts,
  listStylePresets,
  localFileAssetUrl,
  regenerateCues,
  saveStylePreset,
  updateCue,
  updateStyle,
} from "@/lib/tauri";
import type {
  AnimationIn,
  AnimationOut,
  EasingKind,
  KaraokeMode,
  StrokePosition,
  StylePreset,
  SubtitleCue,
  SubtitleStyle,
  UpdateStylePatch,
} from "@/types/studio";

interface CanvasFloatingToolbarProps {
  projectId: string;
  currentCue: SubtitleCue | null;
  style: SubtitleStyle | null;
  onUpdated: () => void;
}

type PopoverMenu = "border" | "styles" | "anim" | "more" | null;

const ANIM_IN_OPTIONS: ReadonlyArray<{ id: AnimationIn; label: string }> = [
  { id: "pop", label: "Pop (Overshoot)" },
  { id: "fade", label: "Fade In" },
  { id: "slide-up", label: "Slide Up" },
  { id: "zoom", label: "Zoom In" },
  { id: "typewriter", label: "Typewriter" },
  { id: "word-reveal", label: "Word Reveal" },
  { id: "bounce", label: "Bounce" },
  { id: "blur-in", label: "Blur In" },
  { id: "none", label: "None" },
];

const ANIM_OUT_OPTIONS: ReadonlyArray<{ id: AnimationOut; label: string }> = [
  { id: "none", label: "None" },
  { id: "fade-out", label: "Fade Out" },
  { id: "slide-down", label: "Slide Down" },
];

const EASING_OPTIONS: ReadonlyArray<{ id: EasingKind; label: string }> = [
  { id: "cubic", label: "Cubic" },
  { id: "back", label: "Back" },
  { id: "elastic", label: "Elastic" },
  { id: "spring", label: "Spring" },
  { id: "linear", label: "Linear" },
];

const KARAOKE_OPTIONS: ReadonlyArray<{ id: KaraokeMode; label: string }> = [
  { id: "highlight-current-word", label: "Highlight Word" },
  { id: "progressive-fill", label: "Progressive Fill" },
  { id: "pop-current", label: "Pop Current" },
  { id: "none", label: "Off" },
];

export function CanvasFloatingToolbar({
  projectId,
  currentCue,
  style,
  onUpdated,
}: CanvasFloatingToolbarProps) {
  const queryClient = useQueryClient();
  const [openMenu, setOpenMenu] = useState<PopoverMenu>(null);
  const [presetNameInput, setPresetNameInput] = useState("");
  const [isRegenerating, setIsRegenerating] = useState(false);

  const { data: fonts = [] } = useQuery({
    queryKey: ["fonts"],
    queryFn: listFonts,
  });

  const { data: presets = [] } = useQuery({
    queryKey: ["style-presets"],
    queryFn: listStylePresets,
  });

  const fontFaceCss = useMemo(() => {
    return fonts
      .map((f) => {
        const family = f.fileName.replace(/\.[^/.]+$/, "");
        const url = localFileAssetUrl(f.path);
        return `@font-face { font-family: '${family}'; src: url('${url}'); }`;
      })
      .join("\n");
  }, [fonts]);

  if (!style) return null;

  const patchStyle = async (patch: UpdateStylePatch) => {
    try {
      await updateStyle(projectId, patch);
      onUpdated();
    } catch (err) {
      toast.error(formatTauriError(err, "Failed to update style"));
    }
  };

  const handleWordsPerCue = async (count: number) => {
    setIsRegenerating(true);
    try {
      await regenerateCues(projectId, count);
      onUpdated();
    } catch (err) {
      toast.error(formatTauriError(err));
    } finally {
      setIsRegenerating(false);
    }
  };

  const handleCueColor = async (hex: string) => {
    if (!currentCue) {
      await patchStyle({ fontColorHex: hex });
      return;
    }
    try {
      await updateCue(currentCue.id, { colorHex: hex });
      onUpdated();
    } catch (err) {
      toast.error(formatTauriError(err));
    }
  };

  const handleApplyPreset = async (preset: StylePreset) => {
    await patchStyle({
      ...preset.style,
    });
    setOpenMenu(null);
  };

  const handleSaveUserPreset = async () => {
    const trimmed = presetNameInput.trim();
    if (!trimmed) return;
    try {
      await saveStylePreset({
        id: trimmed.toLowerCase().replace(/[^a-z0-9-_]+/g, "-"),
        name: trimmed,
        description: "Custom preset",
        builtin: false,
        style: {
          fontFileName: style.fontFileName,
          fontSizePx: style.fontSizePx,
          fontColorHex: style.fontColorHex,
          outlineColorHex: style.outlineColorHex,
          outlineWidthPx: style.outlineWidthPx,
          backgroundBoxOn: style.backgroundBoxOn,
          backgroundColorHex: style.backgroundColorHex,
          backgroundOpacity: style.backgroundOpacity,
          positionX: style.positionX,
          positionY: style.positionY,
          rotationDeg: style.rotationDeg,
          uppercase: style.uppercase,
          bold: style.bold,
          italic: style.italic,
          strokePosition: style.strokePosition,
          animIn: style.animIn,
          animOut: style.animOut,
          animDurationMs: style.animDurationMs,
          easing: style.easing,
          karaokeMode: style.karaokeMode,
          karaokeColorHex: style.karaokeColorHex,
          karaokeScale: style.karaokeScale,
          shadow: style.shadow,
          glow: style.glow,
          gradient: style.gradient,
          lineHeight: style.lineHeight,
          letterSpacing: style.letterSpacing,
          boxRadius: style.boxRadius,
          boxPaddingX: style.boxPaddingX,
          boxPaddingY: style.boxPaddingY,
          maxWidthPct: style.maxWidthPct,
        },
      });
      setPresetNameInput("");
      toast.success(`Saved preset "${trimmed}"`);
      await queryClient.invalidateQueries({ queryKey: ["style-presets"] });
    } catch (err) {
      toast.error(formatTauriError(err));
    }
  };

  const handleResetDefaults = async () => {
    await patchStyle({
      fontColorHex: "#FFFFFF",
      outlineWidthPx: 2,
      outlineColorHex: "#000000",
      strokePosition: "outside",
      backgroundBoxOn: false,
      backgroundColorHex: "#000000",
      backgroundOpacity: 0.5,
      bold: false,
      italic: false,
      uppercase: false,
      fontSizePx: 42,
      positionX: 50,
      positionY: 80,
      rotationDeg: 0,
      animIn: "pop",
      animOut: "none",
      animDurationMs: 300,
      easing: "cubic",
      karaokeMode: "highlight-current-word",
      karaokeColorHex: "#FFE600",
      karaokeScale: 1.1,
      shadow: null,
      glow: null,
      gradient: null,
      lineHeight: 1.25,
      letterSpacing: 0,
      boxRadius: 8,
      maxWidthPct: 88,
      wordsPerCue: 6,
    });
    setOpenMenu(null);
  };

  const activeColor = currentCue?.colorHex ?? style.fontColorHex;

  return (
    <div className="relative z-40 my-3 flex flex-col items-center">
      {fontFaceCss && <style>{fontFaceCss}</style>}
      <div className="flex flex-wrap items-center gap-1.5 p-2 rounded-2xl bg-zinc-900/95 border border-zinc-700/80 shadow-2xl backdrop-blur-xl text-white">
        {/* Color */}
        <div className="px-1.5 flex items-center">
          <input
            type="color"
            value={activeColor}
            onChange={(e) => void handleCueColor(e.target.value)}
            title="Cue / Text Color"
            className="w-6 h-6 rounded-full border-2 border-white/60 bg-transparent cursor-pointer"
          />
        </div>

        <div className="w-px h-6 bg-zinc-800" />

        {/* Words / Cue Slider */}
        <div className="flex items-center gap-2 px-2.5 py-1 rounded-xl bg-zinc-800/80 border border-zinc-700/60 text-xs">
          <Flame className="w-3.5 h-3.5 text-amber-400" />
          <span className="text-[11px] font-bold text-amber-300 w-14 text-center">
            {style.wordsPerCue} {style.wordsPerCue === 1 ? "Word" : "Words"}
          </span>
          <input
            type="range"
            min={1}
            max={15}
            value={style.wordsPerCue}
            disabled={isRegenerating}
            onChange={(e) => void handleWordsPerCue(Number(e.target.value))}
            className="w-20 accent-amber-400 cursor-pointer"
          />
        </div>

        <div className="w-px h-6 bg-zinc-800" />

        {/* Font Picker (with live @font-face preview) */}
        <select
          value={style.fontFileName}
          onChange={(e) => void patchStyle({ fontFileName: e.target.value })}
          style={{
            fontFamily: style.fontFileName
              ? `'${style.fontFileName.replace(/\.[^/.]+$/, "")}', sans-serif`
              : "sans-serif",
          }}
          className="bg-zinc-800 hover:bg-zinc-700 text-white text-xs font-semibold px-3 py-1.5 rounded-xl border border-zinc-700/60 max-w-[165px] cursor-pointer outline-none"
        >
          <option value="">System Default (Segoe UI)</option>
          {fonts.map((f) => (
            <option
              key={f.fileName}
              value={f.fileName}
              style={{ fontFamily: `'${f.fileName.replace(/\.[^/.]+$/, "")}', sans-serif` }}
            >
              {f.displayName}
            </option>
          ))}
        </select>

        {/* Font Size */}
        <div className="flex items-center bg-zinc-800 px-2 py-1 rounded-xl border border-zinc-700/60">
          <input
            type="number"
            min={10}
            max={200}
            value={Math.round(style.fontSizePx)}
            onChange={(e) => {
              const v = Math.min(200, Math.max(10, Number(e.target.value) || 42));
              void patchStyle({ fontSizePx: v });
            }}
            className="w-10 bg-transparent text-white text-xs font-bold text-center font-mono outline-none"
          />
          <span className="text-[10px] text-zinc-400">px</span>
        </div>

        <div className="w-px h-6 bg-zinc-800" />

        {/* Border / Stroke */}
        <div className="relative">
          <button
            type="button"
            onClick={() => setOpenMenu(openMenu === "border" ? null : "border")}
            className={`flex items-center gap-1.5 px-3 py-1.5 rounded-xl text-xs font-medium cursor-pointer ${
              openMenu === "border" ? "bg-indigo-600 text-white" : "hover:bg-zinc-800 text-zinc-200"
            }`}
          >
            <Square className="w-3.5 h-3.5 text-cyan-400" />
            Border
          </button>
          {openMenu === "border" && (
            <div className="absolute top-12 left-1/2 -translate-x-1/2 w-64 p-3.5 bg-zinc-900 border border-zinc-700 rounded-2xl shadow-2xl z-50 space-y-3 text-xs">
              <div className="flex items-center justify-between">
                <span className="text-[10px] font-bold uppercase text-zinc-400">Text Stroke</span>
                <button
                  type="button"
                  onClick={() => void patchStyle({ outlineWidthPx: 0 })}
                  className="px-2 py-0.5 rounded bg-red-500/20 text-red-400 text-[10px] font-bold flex items-center gap-1 cursor-pointer"
                >
                  <Ban className="w-3 h-3" /> None
                </button>
              </div>
              <div className="space-y-1">
                <div className="flex justify-between text-[11px]">
                  <span>Width</span>
                  <span className="font-mono text-cyan-400">{style.outlineWidthPx}px</span>
                </div>
                <input
                  type="range"
                  min={0}
                  max={12}
                  step={0.5}
                  value={style.outlineWidthPx}
                  onChange={(e) => void patchStyle({ outlineWidthPx: Number(e.target.value) })}
                  className="w-full accent-cyan-400"
                />
              </div>
              <div className="grid grid-cols-3 gap-1">
                {(["outside", "inside", "center"] as const satisfies readonly StrokePosition[]).map(
                  (pos) => (
                    <button
                      key={pos}
                      type="button"
                      onClick={() => void patchStyle({ strokePosition: pos })}
                      className={`py-1 rounded-lg text-[10px] font-bold capitalize cursor-pointer ${
                        style.strokePosition === pos
                          ? "bg-cyan-500 text-black"
                          : "bg-zinc-800 text-zinc-300"
                      }`}
                    >
                      {pos}
                    </button>
                  ),
                )}
              </div>
              <div className="flex items-center justify-between pt-1 border-t border-zinc-800">
                <span className="text-[11px]">Stroke Color</span>
                <input
                  type="color"
                  value={style.outlineColorHex}
                  onChange={(e) => void patchStyle({ outlineColorHex: e.target.value })}
                  className="w-6 h-6 bg-transparent cursor-pointer"
                />
              </div>
            </div>
          )}
        </div>

        {/* Presets */}
        <div className="relative">
          <button
            type="button"
            onClick={() => setOpenMenu(openMenu === "styles" ? null : "styles")}
            className={`flex items-center gap-1.5 px-3 py-1.5 rounded-xl text-xs font-medium cursor-pointer ${
              openMenu === "styles" ? "bg-indigo-600 text-white" : "hover:bg-zinc-800 text-zinc-200"
            }`}
          >
            <Sparkles className="w-3.5 h-3.5 text-amber-400" />
            Styles
          </button>
          {openMenu === "styles" && (
            <div className="absolute top-12 left-1/2 -translate-x-1/2 w-72 p-3.5 bg-zinc-900 border border-zinc-700 rounded-2xl shadow-2xl z-50 space-y-2 text-xs max-h-96 overflow-y-auto">
              <span className="text-[10px] font-bold uppercase text-zinc-400 block">
                Built-in &amp; Saved JSON Presets
              </span>
              {presets.map((p) => (
                <button
                  key={`${p.builtin}-${p.id}`}
                  type="button"
                  onClick={() => void handleApplyPreset(p)}
                  className="w-full text-left p-2 rounded-xl bg-zinc-800/80 hover:bg-zinc-700 flex items-center justify-between cursor-pointer"
                >
                  <span className="font-semibold text-white">{p.name}</span>
                  <span className="text-[10px] text-zinc-400">{p.description}</span>
                </button>
              ))}
              <div className="pt-2 border-t border-zinc-800 flex items-center gap-1.5">
                <input
                  type="text"
                  value={presetNameInput}
                  onChange={(e) => setPresetNameInput(e.target.value)}
                  placeholder="Save current as..."
                  className="flex-1 bg-zinc-950 border border-zinc-700 rounded-lg px-2 py-1 text-xs text-white outline-none"
                />
                <button
                  type="button"
                  onClick={() => void handleSaveUserPreset()}
                  className="p-1.5 rounded-lg bg-indigo-600 hover:bg-indigo-500 text-white cursor-pointer"
                  title="Save Preset JSON"
                >
                  <Plus className="w-3.5 h-3.5" />
                </button>
              </div>
            </div>
          )}
        </div>

        {/* Animations + Karaoke + Easing */}
        <div className="relative">
          <button
            type="button"
            onClick={() => setOpenMenu(openMenu === "anim" ? null : "anim")}
            className={`flex items-center gap-1.5 px-3 py-1.5 rounded-xl text-xs font-medium cursor-pointer ${
              openMenu === "anim" ? "bg-indigo-600 text-white" : "hover:bg-zinc-800 text-zinc-200"
            }`}
          >
            <Zap className="w-3.5 h-3.5 text-indigo-400" />
            Animation
          </button>
          {openMenu === "anim" && (
            <div className="absolute top-12 left-1/2 -translate-x-1/2 w-72 p-3.5 bg-zinc-900 border border-zinc-700 rounded-2xl shadow-2xl z-50 space-y-3 text-xs max-h-96 overflow-y-auto">
              <div>
                <span className="text-[10px] font-bold uppercase text-zinc-400 block mb-1">
                  Entrance
                </span>
                <div className="grid grid-cols-2 gap-1">
                  {ANIM_IN_OPTIONS.map((a) => (
                    <button
                      key={a.id}
                      type="button"
                      onClick={() => void patchStyle({ animIn: a.id, animation: a.id })}
                      className={`px-2 py-1.5 rounded-lg text-left flex items-center justify-between cursor-pointer ${
                        style.animIn === a.id
                          ? "bg-indigo-600 text-white font-semibold"
                          : "bg-zinc-800/70 text-zinc-300 hover:bg-zinc-800"
                      }`}
                    >
                      <span className="truncate">{a.label}</span>
                      {style.animIn === a.id && <Check className="w-3 h-3 shrink-0" />}
                    </button>
                  ))}
                </div>
              </div>

              <div className="grid grid-cols-2 gap-2 pt-2 border-t border-zinc-800">
                <div>
                  <span className="text-[10px] font-bold uppercase text-zinc-400 block mb-1">
                    Exit
                  </span>
                  <select
                    value={style.animOut}
                    onChange={(e) => void patchStyle({ animOut: e.target.value as AnimationOut })}
                    className="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-2 py-1 text-[11px] text-white"
                  >
                    {ANIM_OUT_OPTIONS.map((o) => (
                      <option key={o.id} value={o.id}>
                        {o.label}
                      </option>
                    ))}
                  </select>
                </div>
                <div>
                  <span className="text-[10px] font-bold uppercase text-zinc-400 block mb-1">
                    Easing
                  </span>
                  <select
                    value={style.easing}
                    onChange={(e) => void patchStyle({ easing: e.target.value as EasingKind })}
                    className="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-2 py-1 text-[11px] text-white"
                  >
                    {EASING_OPTIONS.map((o) => (
                      <option key={o.id} value={o.id}>
                        {o.label}
                      </option>
                    ))}
                  </select>
                </div>
              </div>

              <div className="pt-2 border-t border-zinc-800 space-y-1.5">
                <span className="text-[10px] font-bold uppercase text-zinc-400 block">
                  Karaoke Mode
                </span>
                <select
                  value={style.karaokeMode}
                  onChange={(e) => void patchStyle({ karaokeMode: e.target.value as KaraokeMode })}
                  className="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-2 py-1.5 text-[11px] text-white"
                >
                  {KARAOKE_OPTIONS.map((o) => (
                    <option key={o.id} value={o.id}>
                      {o.label}
                    </option>
                  ))}
                </select>
              </div>
            </div>
          )}
        </div>

        {/* More: Bold/Italic/CAPS, Box, Shadow, Glow, Gradient, Reset */}
        <div className="relative">
          <button
            type="button"
            onClick={() => setOpenMenu(openMenu === "more" ? null : "more")}
            className={`p-2 rounded-xl cursor-pointer ${
              openMenu === "more" ? "bg-indigo-600 text-white" : "hover:bg-zinc-800 text-zinc-300"
            }`}
          >
            <MoreHorizontal className="w-4 h-4" />
          </button>
          {openMenu === "more" && (
            <div className="absolute top-12 right-0 w-72 p-3.5 bg-zinc-900 border border-zinc-700 rounded-2xl shadow-2xl z-50 space-y-3 text-xs max-h-96 overflow-y-auto">
              <div className="grid grid-cols-3 gap-1.5">
                <button
                  type="button"
                  onClick={() => void patchStyle({ bold: !style.bold })}
                  className={`p-2 rounded-lg border flex items-center justify-center cursor-pointer ${
                    style.bold ? "bg-indigo-600 border-indigo-500" : "bg-zinc-800 border-zinc-700 text-zinc-400"
                  }`}
                >
                  <Bold className="w-3.5 h-3.5" />
                </button>
                <button
                  type="button"
                  onClick={() => void patchStyle({ italic: !style.italic })}
                  className={`p-2 rounded-lg border flex items-center justify-center cursor-pointer ${
                    style.italic ? "bg-indigo-600 border-indigo-500" : "bg-zinc-800 border-zinc-700 text-zinc-400"
                  }`}
                >
                  <Italic className="w-3.5 h-3.5" />
                </button>
                <button
                  type="button"
                  onClick={() => void patchStyle({ uppercase: !style.uppercase })}
                  className={`p-2 rounded-lg border flex items-center justify-center cursor-pointer ${
                    style.uppercase ? "bg-indigo-600 border-indigo-500" : "bg-zinc-800 border-zinc-700 text-zinc-400"
                  }`}
                >
                  <Type className="w-3.5 h-3.5" />
                </button>
              </div>

              <div className="space-y-2 pt-2 border-t border-zinc-800">
                <label className="flex items-center justify-between cursor-pointer">
                  <span>Background Box</span>
                  <input
                    type="checkbox"
                    checked={style.backgroundBoxOn}
                    onChange={(e) => void patchStyle({ backgroundBoxOn: e.target.checked })}
                    className="accent-indigo-500"
                  />
                </label>
                {style.backgroundBoxOn && (
                  <div className="flex items-center gap-2">
                    <input
                      type="color"
                      value={style.backgroundColorHex}
                      onChange={(e) => void patchStyle({ backgroundColorHex: e.target.value })}
                      className="w-6 h-6 bg-transparent cursor-pointer"
                    />
                    <input
                      type="range"
                      min={0.1}
                      max={1}
                      step={0.05}
                      value={style.backgroundOpacity}
                      onChange={(e) => void patchStyle({ backgroundOpacity: Number(e.target.value) })}
                      className="flex-1 accent-indigo-500"
                    />
                  </div>
                )}

                <label className="flex items-center justify-between cursor-pointer">
                  <span>Drop Shadow</span>
                  <input
                    type="checkbox"
                    checked={Boolean(style.shadow?.enabled)}
                    onChange={(e) =>
                      void patchStyle({
                        shadow: e.target.checked
                          ? { enabled: true, colorHex: "#000000", offsetX: 3, offsetY: 3, blur: 4 }
                          : null,
                      })
                    }
                    className="accent-indigo-500"
                  />
                </label>

                <label className="flex items-center justify-between cursor-pointer">
                  <span>Neon Glow</span>
                  <input
                    type="checkbox"
                    checked={Boolean(style.glow?.enabled)}
                    onChange={(e) =>
                      void patchStyle({
                        glow: e.target.checked
                          ? { enabled: true, colorHex: "#00E5FF", radius: 10, intensity: 1 }
                          : null,
                      })
                    }
                    className="accent-indigo-500"
                  />
                </label>

                <label className="flex items-center justify-between cursor-pointer">
                  <span>Gradient Fill</span>
                  <input
                    type="checkbox"
                    checked={Boolean(style.gradient?.enabled)}
                    onChange={(e) =>
                      void patchStyle({
                        gradient: e.target.checked
                          ? { enabled: true, startHex: "#FF7700", endHex: "#FFE600", angleDeg: 45 }
                          : null,
                      })
                    }
                    className="accent-indigo-500"
                  />
                </label>
              </div>

              <div className="pt-2 border-t border-zinc-800">
                <button
                  type="button"
                  onClick={() => void handleResetDefaults()}
                  className="w-full py-2 rounded-xl bg-zinc-800 hover:bg-zinc-700 text-zinc-200 font-semibold flex items-center justify-center gap-1.5 cursor-pointer"
                >
                  <RotateCcw className="w-3.5 h-3.5 text-amber-400" />
                  Reset All to Defaults
                </button>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

