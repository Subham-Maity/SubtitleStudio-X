import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import type {
  BatchFolderResult,
  BatchIndexItem,
  BatchTimestampMode,
  CleanJunkResult,
  DetectedEncoder,
  ExportedSubtitles,
  ExportFormat,
  ExportJob,
  ExportMode,
  ExportQuality,
  FontInfo,
  HealthStatus,
  StylePreset,
  SubtitleCue,
  SubtitleStyle,
  SubtitleTextFormat,
  UpdateStylePatch,
  VideoProject,
  WhisperModelInfo,
} from "@/types/studio";

export function formatTauriError(err: unknown, fallback = "Operation failed"): string {
  if (typeof err === "string") return err;
  if (err instanceof Error) return err.message;
  if (typeof err === "object" && err !== null && "message" in err) {
    const msg = (err as { message?: unknown }).message;
    if (typeof msg === "string") return msg;
  }
  return fallback;
}

export function localFileAssetUrl(filePath: string): string {
  return convertFileSrc(filePath);
}

export async function healthCheck(): Promise<HealthStatus> {
  return invoke<HealthStatus>("health_check");
}

export async function listProjects(): Promise<VideoProject[]> {
  return invoke<VideoProject[]>("list_projects");
}

export async function getProject(id: string): Promise<VideoProject> {
  return invoke<VideoProject>("get_project", { id });
}

export async function importVideo(
  filePath: string,
  language: string | null,
  autoTranscribe = true,
): Promise<VideoProject> {
  return invoke<VideoProject>("import_video", {
    filePath,
    language,
    autoTranscribe,
  });
}

export async function deleteProject(id: string): Promise<void> {
  return invoke<void>("delete_project", { id });
}

export async function retryPipeline(id: string): Promise<void> {
  return invoke<void>("retry_pipeline", { id });
}

export async function retranscribePipeline(
  id: string,
  language: string | null,
): Promise<void> {
  return invoke<void>("retranscribe_pipeline", { id, language });
}

export async function resetPipeline(id: string): Promise<string> {
  return invoke<string>("reset_pipeline", { id });
}

export async function stopAndClearQueue(): Promise<number> {
  return invoke<number>("stop_and_clear_queue");
}

export async function cleanStorageJunk(): Promise<CleanJunkResult> {
  return invoke<CleanJunkResult>("clean_storage_junk");
}

export async function createCue(
  projectId: string,
  text: string,
  startMs: number,
  endMs: number,
  colorHex: string | null = null,
): Promise<SubtitleCue> {
  return invoke<SubtitleCue>("create_cue", {
    projectId,
    text,
    startMs,
    endMs,
    colorHex,
  });
}

export async function updateCue(
  cueId: string,
  patch: {
    text?: string;
    startMs?: number;
    endMs?: number;
    colorHex?: string | null;
  },
): Promise<SubtitleCue> {
  return invoke<SubtitleCue>("update_cue", {
    cueId,
    text: patch.text,
    startMs: patch.startMs,
    endMs: patch.endMs,
    colorHex: patch.colorHex === undefined ? undefined : patch.colorHex,
  });
}

export async function deleteCue(cueId: string): Promise<void> {
  return invoke<void>("delete_cue", { cueId });
}

export async function regenerateCues(
  projectId: string,
  wordsPerCue: number,
): Promise<SubtitleCue[]> {
  return invoke<SubtitleCue[]>("regenerate_cues", { projectId, wordsPerCue });
}

export async function updateStyle(
  projectId: string,
  patch: UpdateStylePatch,
): Promise<SubtitleStyle> {
  return invoke<SubtitleStyle>("update_style", { projectId, patch });
}

export async function exportSubtitlesText(
  projectId: string,
  format: SubtitleTextFormat,
): Promise<ExportedSubtitles> {
  return invoke<ExportedSubtitles>("export_subtitles_text", { projectId, format });
}

export async function saveTextFile(destPath: string, content: string): Promise<void> {
  return invoke<void>("save_text_file", { destPath, content });
}

export async function mergeBatchSubtitles(
  projectIds: string[],
  format: SubtitleTextFormat,
): Promise<ExportedSubtitles> {
  return invoke<ExportedSubtitles>("merge_batch_subtitles", { projectIds, format });
}

export async function listFonts(): Promise<FontInfo[]> {
  return invoke<FontInfo[]>("list_fonts");
}

export async function listStylePresets(): Promise<StylePreset[]> {
  return invoke<StylePreset[]>("list_style_presets");
}

export async function saveStylePreset(preset: StylePreset): Promise<StylePreset> {
  return invoke<StylePreset>("save_style_preset", { preset });
}

export async function renderPreviewFrame(
  projectId: string,
  tMs: number,
  outW: number,
  outH: number,
): Promise<Uint8ClampedArray> {
  const buf = await invoke<ArrayBuffer>("render_preview_frame", {
    projectId,
    tMs: Math.round(tMs),
    outW: Math.max(2, Math.round(outW)),
    outH: Math.max(2, Math.round(outH)),
  });
  return new Uint8ClampedArray(buf);
}

export async function listWhisperModels(): Promise<WhisperModelInfo[]> {
  return invoke<WhisperModelInfo[]>("list_whisper_models");
}

export async function downloadWhisperModel(modelId: string): Promise<string> {
  return invoke<string>("download_whisper_model", { modelId });
}

export async function setPreferredModel(modelId: string | null): Promise<void> {
  return invoke<void>("set_preferred_model", { modelId });
}

export async function unloadWhisperModel(): Promise<void> {
  return invoke<void>("unload_whisper_model");
}

export async function detectExportEncoder(format: ExportFormat): Promise<DetectedEncoder> {
  return invoke<DetectedEncoder>("detect_export_encoder", { format });
}

export async function startExport(payload: {
  projectId: string;
  mode: ExportMode;
  format: ExportFormat;
  quality: ExportQuality;
  backgroundHex?: string;
  includeAudio: boolean;
  destPath?: string;
}): Promise<ExportJob> {
  return invoke<ExportJob>("start_export", { payload });
}

export async function cancelExport(jobId: string): Promise<void> {
  return invoke<void>("cancel_export", { jobId });
}

export async function revealInExplorer(path: string): Promise<void> {
  return invoke<void>("reveal_in_explorer", { path });
}

export async function scanBatchFolder(folderPath: string): Promise<BatchIndexItem[]> {
  return invoke<BatchIndexItem[]>("scan_batch_folder", { folderPath });
}

export async function startBatchFolder(
  folderPath: string,
  timestampMode: BatchTimestampMode,
  forceRestart: boolean,
): Promise<BatchFolderResult> {
  return invoke<BatchFolderResult>("start_batch_folder", {
    folderPath,
    timestampMode,
    forceRestart,
  });
}

export async function cancelBatchFolder(): Promise<void> {
  return invoke<void>("cancel_batch_folder");
}
