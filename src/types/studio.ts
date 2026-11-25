export type PipelineStatus =
  | "UPLOADED"
  | "EXTRACTING_AUDIO"
  | "TRANSCRIBING"
  | "TRANSCRIBED"
  | "FAILED";

export type ExportMode = "OVERLAY" | "CAPTIONS_ONLY";
export type ExportFormat = "MP4_H265" | "MOV";
export type ExportQuality = "fast" | "balanced" | "quality";
export type ExportStatus = "QUEUED" | "RENDERING" | "DONE" | "FAILED";

export type AnimationIn =
  | "none"
  | "fade"
  | "pop"
  | "slide-up"
  | "zoom"
  | "typewriter"
  | "word-reveal"
  | "bounce"
  | "blur-in";

export type AnimationOut = "none" | "fade-out" | "slide-down";
export type EasingKind = "linear" | "cubic" | "back" | "elastic" | "spring";
export type KaraokeMode =
  | "none"
  | "highlight-current-word"
  | "progressive-fill"
  | "pop-current";
export type StrokePosition = "outside" | "inside" | "center";
export type SubtitleTextFormat = "srt" | "text";
export type BatchTimestampMode = "with" | "without" | "both";

export interface ShadowConfig {
  enabled: boolean;
  colorHex: string;
  offsetX: number;
  offsetY: number;
  blur: number;
}

export interface GlowConfig {
  enabled: boolean;
  colorHex: string;
  radius: number;
  intensity: number;
}

export interface GradientConfig {
  enabled: boolean;
  startHex: string;
  endHex: string;
  angleDeg: number;
}

export interface TranscriptWord {
  id: string;
  videoProjectId: string;
  word: string;
  startMs: number;
  endMs: number;
  order: number;
}

export interface SubtitleCue {
  id: string;
  videoProjectId: string;
  text: string;
  startMs: number;
  endMs: number;
  order: number;
  colorHex: string | null;
  edited: boolean;
  wordStartIdx: number | null;
  wordEndIdx: number | null;
}

export interface SubtitleStyle {
  id: string;
  videoProjectId: string;
  fontFileName: string;
  fontSizePx: number;
  fontColorHex: string;
  outlineColorHex: string;
  outlineWidthPx: number;
  backgroundBoxOn: boolean;
  backgroundColorHex: string;
  backgroundOpacity: number;
  position: string;
  verticalOffsetPct: number;
  positionX: number;
  positionY: number;
  rotationDeg: number;
  animation: string;
  wordsPerCue: number;
  uppercase: boolean;
  bold: boolean;
  italic: boolean;
  strokePosition: StrokePosition;
  animIn: AnimationIn;
  animOut: AnimationOut;
  animDurationMs: number;
  easing: EasingKind;
  karaokeMode: KaraokeMode;
  karaokeColorHex: string;
  karaokeScale: number;
  shadow: ShadowConfig | null;
  glow: GlowConfig | null;
  gradient: GradientConfig | null;
  lineHeight: number;
  letterSpacing: number;
  boxRadius: number;
  boxPaddingX: number;
  boxPaddingY: number;
  maxWidthPct: number;
}

export type UpdateStylePatch = Partial<{
  fontFileName: string;
  fontSizePx: number;
  fontColorHex: string;
  outlineColorHex: string;
  outlineWidthPx: number;
  backgroundBoxOn: boolean;
  backgroundColorHex: string;
  backgroundOpacity: number;
  position: string;
  verticalOffsetPct: number;
  positionX: number;
  positionY: number;
  rotationDeg: number;
  animation: string;
  wordsPerCue: number;
  uppercase: boolean;
  bold: boolean;
  italic: boolean;
  strokePosition: StrokePosition;
  animIn: AnimationIn;
  animOut: AnimationOut;
  animDurationMs: number;
  easing: EasingKind;
  karaokeMode: KaraokeMode;
  karaokeColorHex: string;
  karaokeScale: number;
  shadow: ShadowConfig | null;
  glow: GlowConfig | null;
  gradient: GradientConfig | null;
  lineHeight: number;
  letterSpacing: number;
  boxRadius: number;
  boxPaddingX: number;
  boxPaddingY: number;
  maxWidthPct: number;
}>;

export interface ExportJob {
  id: string;
  videoProjectId: string;
  mode: ExportMode;
  backgroundHex: string | null;
  includeAudio: boolean;
  format: ExportFormat;
  quality: ExportQuality;
  encoder: string | null;
  status: ExportStatus;
  progressPct: number;
  outputPath: string | null;
  errorMessage: string | null;
  createdAt: string;
}

export interface VideoProject {
  id: string;
  originalFilename: string;
  storagePath: string;
  durationMs: number;
  width: number;
  height: number;
  fps: number;
  status: PipelineStatus;
  language: string | null;
  errorMessage: string | null;
  createdAt: string;
  updatedAt: string;
  words: TranscriptWord[];
  cues: SubtitleCue[];
  style: SubtitleStyle | null;
  exportJobs: ExportJob[];
  cueCount: number;
}

export interface FontInfo {
  fileName: string;
  displayName: string;
  path: string;
}

export interface HealthStatus {
  appDataRoot: string;
  modelsDir: string;
  userFontsDir: string;
  bundledFontsDir: string;
  presetsDir: string;
  ffmpegPath: string;
  ffprobePath: string;
  vulkanAvailable: boolean;
  version: string;
}

export interface CleanJunkResult {
  freedMb: number;
  deletedFilesCount: number;
  message: string;
}

export interface ExportedSubtitles {
  filename: string;
  content: string;
  format: SubtitleTextFormat;
}

export interface WhisperModelInfo {
  id: string;
  label: string;
  fileName: string;
  downloadUrl: string;
  approxSizeMb: number;
  downloaded: boolean;
  localPath: string;
  sizeBytes: number;
  vramNote: string;
}

export interface ModelDownloadProgress {
  modelId: string;
  downloadedBytes: number;
  totalBytes: number | null;
  pct: number;
  done: boolean;
}

export interface PipelineProgressEvent {
  projectId: string;
  eventType: string;
  stage: string;
  pct: number;
  elapsedMs: number;
  language: string | null;
  device: string | null;
  wordCount: number | null;
  logMessage: string | null;
  message: string | null;
}

export interface DetectedEncoder {
  codec: string;
  isHardware: boolean;
  displayName: string;
  fallbackNotice: string | null;
}

export interface ExportProgressEvent {
  jobId: string;
  projectId: string;
  eventType: string;
  progressPct: number;
  encoder: string;
  isHardware: boolean;
  fallbackNotice: string | null;
  outputPath: string | null;
  message: string | null;
}

export interface StylePreset {
  id: string;
  name: string;
  description: string;
  builtin: boolean;
  style: Omit<
    SubtitleStyle,
    "id" | "videoProjectId" | "position" | "verticalOffsetPct" | "animation" | "wordsPerCue"
  >;
}

export interface BatchIndexItem {
  id: number;
  filename: string;
  filePath: string;
  sizeBytes: number;
  status: string;
  detectedLanguage: string | null;
  isHindi: boolean;
  translatedToEnglish: boolean;
  transcriptTxtPath: string | null;
  cueCount: number;
  wordCount: number;
  error: string | null;
  processedAt: string | null;
}

export interface BatchProgressEvent {
  currentIndex: number;
  total: number;
  filename: string;
  status: string;
  pct: number;
  logMessage: string;
  mergedTxtPath: string | null;
}

export interface BatchFolderResult {
  items: BatchIndexItem[];
  mergedTxtPath: string;
}

export type AppRoute =
  | { view: "home" }
  | { view: "editor"; projectId: string }
  | { view: "batch" };

export const LANGUAGE_OPTIONS: ReadonlyArray<{
  code: string;
  name: string;
  desc: string;
}> = [
  {
    code: "en",
    name: "Translate Audio to English (en)",
    desc: "Translates Hindi or any spoken language into English subtitles",
  },
  {
    code: "hi-strict",
    name: "Strict Hindi (100% Devanagari - hi-strict)",
    desc: "Devanagari script prompt for Hindi",
  },
  {
    code: "hi",
    name: "Standard Hindi (Devanagari - hi)",
    desc: "Guarantees Devanagari script for Hindi",
  },
  {
    code: "hi-roman",
    name: "Hinglish (Roman Script - hi-roman)",
    desc: "Latin script for Hindi (e.g. Namaste dosto)",
  },
  {
    code: "auto",
    name: "Auto-detect Original Language",
    desc: "Automatic language identification",
  },
  { code: "ur", name: "Urdu (ur)", desc: "Urdu script" },
  { code: "es", name: "Spanish (es)", desc: "Spanish language" },
  { code: "fr", name: "French (fr)", desc: "French language" },
];
