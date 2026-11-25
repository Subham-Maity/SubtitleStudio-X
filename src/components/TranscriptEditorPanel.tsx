import { useState } from "react";
import { save as saveFileDialog } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  Check,
  Download,
  Edit3,
  Globe,
  Languages,
  Loader2,
  Play,
  Plus,
  RefreshCw,
  Trash2,
} from "lucide-react";
import { toast } from "sonner";
import {
  createCue,
  deleteCue,
  exportSubtitlesText,
  formatTauriError,
  regenerateCues,
  retranscribePipeline,
  saveTextFile,
  updateCue,
} from "@/lib/tauri";
import { LANGUAGE_OPTIONS, type SubtitleCue, type SubtitleTextFormat } from "@/types/studio";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";

interface TranscriptEditorPanelProps {
  projectId: string;
  cues: SubtitleCue[];
  currentTimeMs: number;
  wordsPerCue: number;
  onSeek: (ms: number) => void;
  onUpdated: () => void;
}

function formatMs(ms: number): string {
  const totalSec = Math.floor(ms / 1000);
  const m = Math.floor(totalSec / 60);
  const s = totalSec % 60;
  const dec = Math.floor((ms % 1000) / 100);
  return `${m}:${s.toString().padStart(2, "0")}.${dec}`;
}

export function TranscriptEditorPanel({
  projectId,
  cues,
  currentTimeMs,
  wordsPerCue,
  onSeek,
  onUpdated,
}: TranscriptEditorPanelProps) {
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editText, setEditText] = useState("");
  const [isAdding, setIsAdding] = useState(false);
  const [newCueText, setNewCueText] = useState("");
  const [selectedLang, setSelectedLang] = useState("hi");
  const [showWarnModal, setShowWarnModal] = useState(false);
  const [busy, setBusy] = useState(false);

  const hasEdited = cues.some((c) => c.edited);

  const handleSaveEdit = async (cueId: string) => {
    try {
      await updateCue(cueId, { text: editText });
      setEditingId(null);
      onUpdated();
    } catch (err) {
      toast.error(formatTauriError(err));
    }
  };

  const handleAddCue = async () => {
    const trimmed = newCueText.trim();
    if (!trimmed) return;
    const last = cues[cues.length - 1];
    const startMs = last ? last.endMs + 100 : 0;
    try {
      await createCue(projectId, trimmed, startMs, startMs + 2000, null);
      setNewCueText("");
      setIsAdding(false);
      onUpdated();
    } catch (err) {
      toast.error(formatTauriError(err));
    }
  };

  const handleRegenerate = async () => {
    setShowWarnModal(false);
    setBusy(true);
    try {
      await regenerateCues(projectId, wordsPerCue);
      onUpdated();
      toast.success("Regenerated cues from word timestamps");
    } catch (err) {
      toast.error(formatTauriError(err));
    } finally {
      setBusy(false);
    }
  };

  const handleRetranscribe = async () => {
    setBusy(true);
    try {
      await retranscribePipeline(projectId, selectedLang);
      onUpdated();
    } catch (err) {
      toast.error(formatTauriError(err));
    } finally {
      setBusy(false);
    }
  };

  const handleExportText = async (format: SubtitleTextFormat) => {
    try {
      const res = await exportSubtitlesText(projectId, format);
      const ext = format === "srt" ? "srt" : "txt";
      const dest = await saveFileDialog({
        defaultPath: res.filename,
        filters: [{ name: format.toUpperCase(), extensions: [ext] }],
      });
      if (!dest) return;
      await saveTextFile(dest, res.content);
      toast.success(`Saved ${ext.toUpperCase()} subtitles`);
    } catch (err) {
      toast.error(formatTauriError(err));
    }
  };

  return (
    <Card className="p-5 flex flex-col h-[520px]">
      <div className="flex flex-wrap items-center justify-between gap-3 pb-4 border-b border-zinc-800">
        <div>
          <h3 className="text-sm font-bold text-white flex items-center gap-2">
            <Edit3 className="w-4 h-4 text-indigo-400" />
            Transcript Cues ({cues.length})
          </h3>
          <p className="text-[11px] text-zinc-400">
            Click timestamp to seek &bull; Double-click text to edit
          </p>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          <div className="flex items-center gap-1.5 bg-zinc-900 border border-zinc-700 rounded-xl px-2.5 py-1.5">
            <Languages className="w-3.5 h-3.5 text-indigo-400" />
            <select
              value={selectedLang}
              onChange={(e) => setSelectedLang(e.target.value)}
              className="bg-transparent text-xs font-semibold text-zinc-200 outline-none cursor-pointer max-w-[180px]"
            >
              {LANGUAGE_OPTIONS.map((opt) => (
                <option key={opt.code} value={opt.code} className="bg-zinc-900 text-white">
                  {opt.name}
                </option>
              ))}
            </select>
          </div>

          <Button size="sm" disabled={busy} onClick={() => void handleRetranscribe()}>
            {busy ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <Globe className="w-3.5 h-3.5" />}
            Re-transcribe
          </Button>
          <Button size="sm" variant="secondary" onClick={() => setIsAdding(!isAdding)}>
            <Plus className="w-3.5 h-3.5" /> Add Cue
          </Button>
          <Button
            size="sm"
            variant="secondary"
            disabled={busy}
            onClick={() => (hasEdited ? setShowWarnModal(true) : void handleRegenerate())}
          >
            <RefreshCw className="w-3.5 h-3.5" /> Regenerate
          </Button>
          <Button size="sm" variant="outline" onClick={() => void handleExportText("srt")}>
            <Download className="w-3.5 h-3.5" /> .SRT
          </Button>
          <Button size="sm" variant="outline" onClick={() => void handleExportText("text")}>
            <Download className="w-3.5 h-3.5" /> .TXT
          </Button>
        </div>
      </div>

      {isAdding && (
        <div className="my-3 p-3 rounded-xl bg-zinc-900 border border-indigo-500/40 flex items-center gap-2">
          <input
            type="text"
            value={newCueText}
            onChange={(e) => setNewCueText(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void handleAddCue();
            }}
            placeholder="Type new subtitle cue..."
            className="flex-1 bg-zinc-950 border border-zinc-700 rounded-lg px-3 py-1.5 text-xs text-white outline-none"
          />
          <Button size="sm" onClick={() => void handleAddCue()}>
            Save Cue
          </Button>
        </div>
      )}

      <div className="flex-1 overflow-y-auto space-y-2.5 pt-3 pr-1">
        {cues.length === 0 ? (
          <div className="h-full flex flex-col items-center justify-center text-xs text-zinc-500">
            No cues yet. Choose a language above and click Re-transcribe, or Add Cue manually.
          </div>
        ) : (
          cues.map((cue) => {
            const isActive = currentTimeMs >= cue.startMs && currentTimeMs <= cue.endMs;
            const isEditing = editingId === cue.id;
            return (
              <div
                key={cue.id}
                onDoubleClick={() => {
                  setEditingId(cue.id);
                  setEditText(cue.text);
                }}
                className={`p-3 rounded-xl border transition-all ${
                  isActive
                    ? "border-indigo-500 bg-indigo-500/10"
                    : "border-zinc-800 bg-zinc-900/50 hover:border-zinc-700"
                }`}
              >
                <div className="flex items-center justify-between mb-1.5">
                  <button
                    type="button"
                    onClick={() => onSeek(cue.startMs)}
                    className="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-md bg-zinc-800 hover:bg-indigo-600 text-zinc-300 hover:text-white text-[11px] font-mono cursor-pointer"
                  >
                    <Play className="w-3 h-3 fill-current" />
                    {formatMs(cue.startMs)} &rarr; {formatMs(cue.endMs)}
                  </button>
                  <div className="flex items-center gap-2">
                    <input
                      type="color"
                      value={cue.colorHex ?? "#FFFFFF"}
                      onChange={(e) => {
                        void updateCue(cue.id, { colorHex: e.target.value }).then(onUpdated);
                      }}
                      title="Per-cue color override"
                      className="w-5 h-5 bg-transparent cursor-pointer"
                    />
                    <button
                      type="button"
                      onClick={() => {
                        void deleteCue(cue.id).then(onUpdated);
                      }}
                      className="p-1 rounded text-zinc-500 hover:text-red-400 cursor-pointer"
                    >
                      <Trash2 className="w-3.5 h-3.5" />
                    </button>
                  </div>
                </div>

                {isEditing ? (
                  <div className="flex items-center gap-2">
                    <textarea
                      rows={2}
                      value={editText}
                      onChange={(e) => setEditText(e.target.value)}
                      className="flex-1 bg-zinc-950 border border-indigo-500 rounded-lg p-2 text-xs text-white outline-none resize-none"
                    />
                    <Button size="icon" onClick={() => void handleSaveEdit(cue.id)}>
                      <Check className="w-4 h-4" />
                    </Button>
                  </div>
                ) : (
                  <p className="text-xs font-medium text-zinc-100" style={{ color: cue.colorHex ?? undefined }}>
                    {cue.text}
                  </p>
                )}
              </div>
            );
          })
        )}
      </div>

      {showWarnModal && (
        <div className="fixed inset-0 z-50 bg-black/80 backdrop-blur-sm flex items-center justify-center p-4">
          <Card className="max-w-md w-full p-6 space-y-4 bg-zinc-950 border-zinc-800">
            <div className="flex items-center gap-2.5 text-amber-400">
              <AlertTriangle className="w-5 h-5" />
              <h4 className="text-sm font-bold text-white">Overwrite Manual Edits?</h4>
            </div>
            <p className="text-xs text-zinc-400">
              Regenerating cues from word timestamps will replace your manually edited cue text.
            </p>
            <div className="flex justify-end gap-2">
              <Button variant="secondary" onClick={() => setShowWarnModal(false)}>
                Cancel
              </Button>
              <Button onClick={() => void handleRegenerate()}>Confirm Regenerate</Button>
            </div>
          </Card>
        </div>
      )}
    </Card>
  );
}

