import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { motion } from "motion/react";
import {
  AlertCircle,
  Clock,
  Cpu,
  Loader2,
  Music,
  RefreshCw,
  ShieldCheck,
  Terminal,
} from "lucide-react";
import { toast } from "sonner";
import { formatTauriError, resetPipeline, retryPipeline } from "@/lib/tauri";
import type { PipelineProgressEvent, PipelineStatus } from "@/types/studio";
import { Button } from "@/components/ui/button";
import { Badge, Card } from "@/components/ui/card";

interface PipelineProgressBannerProps {
  projectId: string;
  status: PipelineStatus;
  errorMessage: string | null;
  onCompleted: () => void;
}

interface LogLine {
  ts: string;
  message: string;
}

export function PipelineProgressBanner({
  projectId,
  status,
  errorMessage,
  onCompleted,
}: PipelineProgressBannerProps) {
  const [stage, setStage] = useState<string>(status);
  const [pct, setPct] = useState<number>(10);
  const [elapsedMs, setElapsedMs] = useState<number>(0);
  const [deviceInfo, setDeviceInfo] = useState<string>("Whisper Vulkan / CPU");
  const [detectedLang, setDetectedLang] = useState<string | null>(null);
  const [errMsg, setErrMsg] = useState<string | null>(errorMessage);
  const [logs, setLogs] = useState<LogLine[]>([]);
  const logBoxRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    setStage(status);
    setErrMsg(errorMessage);
  }, [status, errorMessage]);

  useEffect(() => {
    if (stage === "TRANSCRIBED" || stage === "FAILED" || stage === "UPLOADED") return;
    const start = Date.now() - elapsedMs;
    const timer = window.setInterval(() => {
      setElapsedMs(Date.now() - start);
    }, 1000);
    return () => window.clearInterval(timer);
  }, [stage, elapsedMs]);

  useEffect(() => {
    if (logBoxRef.current) {
      logBoxRef.current.scrollTop = logBoxRef.current.scrollHeight;
    }
  }, [logs]);

  useEffect(() => {
    const unlisten = listen<PipelineProgressEvent>("pipeline://progress", (ev) => {
      const p = ev.payload;
      if (p.projectId !== projectId) return;
      if (p.pct > 0) setPct(p.pct);
      if (p.elapsedMs > 0) setElapsedMs(p.elapsedMs);
      if (p.device) setDeviceInfo(p.device);
      if (p.language) setDetectedLang(p.language);
      if (p.logMessage) {
        const ts = new Date().toLocaleTimeString();
        setLogs((prev) => [...prev.slice(-80), { ts, message: p.logMessage ?? "" }]);
      }
      if (p.eventType === "done" || p.stage === "TRANSCRIBED") {
        setStage("TRANSCRIBED");
        setPct(100);
        onCompleted();
      } else if (p.eventType === "error") {
        setStage("FAILED");
        setErrMsg(p.message ?? "Pipeline failed");
      } else {
        setStage(p.stage);
      }
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [projectId, onCompleted]);

  if (stage === "TRANSCRIBED") return null;

  const handleStopAndReset = async () => {
    try {
      await resetPipeline(projectId);
      setStage("UPLOADED");
      onCompleted();
    } catch (err) {
      toast.error(formatTauriError(err));
    }
  };

  const handleRetry = async () => {
    setLogs([]);
    setErrMsg(null);
    setStage("EXTRACTING_AUDIO");
    setPct(10);
    try {
      await retryPipeline(projectId);
    } catch (err) {
      toast.error(formatTauriError(err));
    }
  };

  const sec = Math.floor(elapsedMs / 1000);
  const elapsedLabel = `${String(Math.floor(sec / 60)).padStart(2, "0")}:${String(sec % 60).padStart(2, "0")}s`;

  return (
    <Card className="p-5 space-y-4 border-indigo-500/30 bg-zinc-900/70">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex items-center gap-3">
          {stage === "FAILED" ? (
            <div className="p-2.5 rounded-xl bg-red-500/15 text-red-400">
              <AlertCircle className="w-5 h-5" />
            </div>
          ) : (
            <div className="p-2.5 rounded-xl bg-indigo-500/15 text-indigo-400">
              {stage === "EXTRACTING_AUDIO" ? (
                <Music className="w-5 h-5 animate-pulse" />
              ) : (
                <Loader2 className="w-5 h-5 animate-spin" />
              )}
            </div>
          )}
          <div>
            <div className="flex items-center gap-2">
              <h4 className="text-sm font-bold text-white">
                {stage === "FAILED"
                  ? "Transcription Error"
                  : stage === "EXTRACTING_AUDIO"
                    ? "Extracting 16kHz Audio"
                    : stage === "TRANSCRIBING"
                      ? "Whisper AI Transcription"
                      : "Ready to Transcribe"}
              </h4>
              {stage !== "FAILED" && <Badge variant="default">{pct}%</Badge>}
            </div>
            <p className="text-xs text-zinc-400">
              {stage === "FAILED"
                ? errMsg ?? "Pipeline failed"
                : `Language: ${detectedLang ?? "auto-detect"}`}
            </p>
          </div>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          <Badge variant="default">
            <Clock className="w-3 h-3" /> {elapsedLabel}
          </Badge>
          <Badge variant="success">
            <ShieldCheck className="w-3 h-3" /> {deviceInfo}
          </Badge>
          {stage !== "FAILED" && stage !== "UPLOADED" && (
            <Button size="sm" variant="destructive" onClick={() => void handleStopAndReset()}>
              Stop &amp; Clear
            </Button>
          )}
          {(stage === "FAILED" || stage === "UPLOADED") && (
            <Button size="sm" onClick={() => void handleRetry()}>
              <RefreshCw className="w-3.5 h-3.5" />
              {stage === "FAILED" ? "Retry Pipeline" : "Start Transcription"}
            </Button>
          )}
        </div>
      </div>

      {stage !== "FAILED" && stage !== "UPLOADED" && (
        <div className="w-full h-2.5 bg-zinc-800 rounded-full overflow-hidden">
          <motion.div
            initial={{ scaleX: 0 }}
            animate={{ scaleX: Math.max(0.02, Math.min(1, pct / 100)) }}
            transition={{ type: "spring", stiffness: 140, damping: 24 }}
            className="h-full w-full origin-left bg-gradient-to-r from-indigo-500 via-purple-500 to-emerald-400"
          />
        </div>
      )}

      <div className="rounded-xl bg-zinc-950 border border-zinc-800 p-3 space-y-1.5">
        <div className="flex items-center justify-between text-[11px] font-mono text-zinc-400 border-b border-zinc-800 pb-1.5">
          <span className="flex items-center gap-1.5 font-semibold text-zinc-300">
            <Terminal className="w-3.5 h-3.5 text-indigo-400" />
            Live Pipeline Telemetry
          </span>
          <Cpu className="w-3.5 h-3.5 text-zinc-500" />
        </div>
        <div ref={logBoxRef} className="max-h-28 overflow-y-auto font-mono text-[11px] space-y-1 text-zinc-300">
          {logs.length === 0 ? (
            <p className="text-zinc-500 italic">Waiting for pipeline events...</p>
          ) : (
            logs.map((l, idx) => (
              <div key={idx}>
                <span className="text-zinc-500">[{l.ts}]</span>{" "}
                <span className="text-indigo-400">&gt;</span> {l.message}
              </div>
            ))
          )}
        </div>
      </div>
    </Card>
  );
}

