import { useEffect } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { AnimatePresence, MotionConfig, motion } from "motion/react";
import { Cpu, Film, FolderOpen, Layers, Sparkles, Subtitles } from "lucide-react";
import { healthCheck, revealInExplorer } from "@/lib/tauri";
import { useAppStore } from "@/stores/appStore";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/card";
import { HomeView } from "@/components/HomeView";
import { EditorView } from "@/components/EditorView";
import { BatchView } from "@/components/BatchView";
import { ModelsModal } from "@/components/ModelsModal";

const GITHUB_PROFILE_URL = "https://github.com/Subham-Maity";
const INSTAGRAM_PROFILE_URL = "https://www.instagram.com/subham_xam/";

async function openExternalProfile(url: string) {
  try {
    await openUrl(url);
  } catch {
    window.open(url, "_blank", "noopener,noreferrer");
  }
}

export function App() {
  const queryClient = useQueryClient();
  const { route, setRoute, modelsModalOpen, setModelsModalOpen } = useAppStore();

  const { data: health } = useQuery({
    queryKey: ["health"],
    queryFn: healthCheck,
  });

  useEffect(() => {
    const unlistenFonts = listen("fonts://updated", () => {
      void queryClient.invalidateQueries({ queryKey: ["fonts"] });
    });
    return () => {
      void unlistenFonts.then((fn) => fn());
    };
  }, [queryClient]);

  return (
    <MotionConfig reducedMotion="user">
      <div className="min-h-screen flex flex-col bg-zinc-950 text-zinc-100">
        <header className="sticky top-0 z-40 border-b border-zinc-800/80 bg-zinc-950/85 backdrop-blur-xl px-6 py-3 flex items-center justify-between gap-4">
          <motion.div
            whileHover={{ scale: 1.015 }}
            whileTap={{ scale: 0.985 }}
            transition={{ type: "spring", stiffness: 380, damping: 28 }}
            className="flex items-center gap-3 cursor-pointer"
            onClick={() => setRoute({ view: "home" })}
          >
            <div className="h-9 w-9 rounded-xl bg-indigo-600/20 border border-indigo-500/35 flex items-center justify-center text-indigo-400 shadow-[0_0_16px_rgba(99,102,241,0.28)]">
              <Subtitles className="h-5 w-5" />
            </div>
            <div>
              <div className="flex items-center gap-2">
                <h1 className="text-sm font-bold tracking-tight text-white">SubtitleStudio X</h1>
                <Badge variant="default">v{health?.version ?? "0.1.0"}</Badge>
                {health && (
                  <Badge variant={health.vulkanAvailable ? "success" : "warning"}>
                    {health.vulkanAvailable ? "GPU Active" : "CPU Mode"}
                  </Badge>
                )}
              </div>
              <p className="text-[11px] text-zinc-400">
                Offline Rust + Skia + Whisper + FFmpeg Studio
              </p>
            </div>
          </motion.div>

          <nav className="flex items-center gap-2 flex-wrap justify-end">
            <motion.div
              whileHover={{ scale: 1.02 }}
              transition={{ type: "spring", stiffness: 340, damping: 26 }}
              className="hidden md:flex items-center gap-1 rounded-full border border-indigo-500/40 bg-indigo-950/35 px-2.5 py-1 shadow-[0_0_16px_rgba(99,102,241,0.3)] hover:shadow-[0_0_24px_rgba(129,140,248,0.5)] transition-shadow"
            >
              <Sparkles className="h-3 w-3 text-indigo-400 shrink-0" />
              <span className="text-[11px] text-zinc-400">by</span>
              <button
                type="button"
                onClick={() => void openExternalProfile(GITHUB_PROFILE_URL)}
                title="Subham-Maity on GitHub"
                className="inline-flex items-center gap-1.5 text-[11px] font-semibold text-indigo-200 hover:text-white transition-colors cursor-pointer px-1"
              >
                <svg viewBox="0 0 24 24" className="h-3.5 w-3.5 fill-current" aria-hidden="true">
                  <path d="M12 .5C5.65.5.5 5.65.5 12a11.5 11.5 0 0 0 7.86 10.92c.58.1.79-.25.79-.56v-2.16c-3.2.7-3.87-1.36-3.87-1.36-.52-1.33-1.28-1.68-1.28-1.68-1.05-.72.08-.7.08-.7 1.16.08 1.77 1.2 1.77 1.2 1.03 1.76 2.7 1.25 3.36.96.1-.75.4-1.26.73-1.55-2.55-.29-5.24-1.28-5.24-5.68 0-1.26.45-2.28 1.19-3.09-.12-.29-.52-1.47.11-3.06 0 0 .97-.31 3.17 1.18a11.02 11.02 0 0 1 5.78 0c2.2-1.49 3.17-1.18 3.17-1.18.63 1.59.23 2.77.11 3.06.74.81 1.19 1.83 1.19 3.09 0 4.41-2.69 5.39-5.26 5.67.41.36.78 1.06.78 2.15v3.19c0 .31.21.67.8.56A11.5 11.5 0 0 0 23.5 12C23.5 5.65 18.35.5 12 .5Z" />
                </svg>
                Subham-Maity
              </button>
              <span className="text-zinc-700">|</span>
              <button
                type="button"
                onClick={() => void openExternalProfile(INSTAGRAM_PROFILE_URL)}
                title="Instagram @subham_xam"
                className="inline-flex items-center gap-1 text-[11px] font-medium text-pink-300/90 hover:text-pink-200 transition-colors cursor-pointer px-1"
              >
                <svg
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="2"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  className="h-3.5 w-3.5"
                  aria-hidden="true"
                >
                  <rect width="20" height="20" x="2" y="2" rx="5" ry="5" />
                  <path d="M16 11.37A4 4 0 1 1 12.63 8 4 4 0 0 1 16 11.37z" />
                  <line x1="17.5" x2="17.51" y1="6.5" y2="6.5" />
                </svg>
              </button>
            </motion.div>

            <div className="flex items-center gap-1 rounded-xl bg-zinc-900/80 border border-zinc-800/90 p-0.5">
              {(
                [
                  { id: "home", label: "Projects", icon: Film },
                  { id: "batch", label: "Batch Translate", icon: Layers },
                ] as const
              ).map((item) => {
                const active = route.view === item.id;
                const Icon = item.icon;
                return (
                  <button
                    key={item.id}
                    type="button"
                    onClick={() => setRoute({ view: item.id })}
                    className={`relative inline-flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-[11px] font-semibold transition-colors cursor-pointer ${
                      active ? "text-white" : "text-zinc-400 hover:text-zinc-200"
                    }`}
                  >
                    {active && (
                      <motion.span
                        layoutId="nav-active-pill"
                        transition={{ type: "spring", stiffness: 380, damping: 30 }}
                        className="absolute inset-0 rounded-lg bg-indigo-600 shadow-md shadow-indigo-600/25"
                      />
                    )}
                    <Icon className="relative z-10 h-3.5 w-3.5" />
                    <span className="relative z-10">{item.label}</span>
                  </button>
                );
              })}
            </div>

            <Button variant="secondary" size="sm" onClick={() => setModelsModalOpen(true)}>
              <Cpu className="h-3.5 w-3.5 text-indigo-400" />
              AI Models
            </Button>
            {health?.userFontsDir && (
              <Button
                variant="ghost"
                size="sm"
                onClick={() => void revealInExplorer(health.userFontsDir)}
                title="Open Custom Fonts Folder"
              >
                <FolderOpen className="h-3.5 w-3.5" />
                Fonts
              </Button>
            )}
          </nav>
        </header>

        <main className="flex-1 max-w-6xl w-full mx-auto p-6">
          <AnimatePresence mode="wait">
            <motion.div
              key={route.view === "editor" ? `editor-${route.projectId}` : route.view}
              initial={{ opacity: 0, y: 6 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -4 }}
              transition={{ duration: 0.18, ease: [0.16, 1, 0.3, 1] }}
            >
              {route.view === "home" && <HomeView />}
              {route.view === "editor" && <EditorView projectId={route.projectId} />}
              {route.view === "batch" && <BatchView />}
            </motion.div>
          </AnimatePresence>
        </main>

        <ModelsModal open={modelsModalOpen} onClose={() => setModelsModalOpen(false)} />
      </div>
    </MotionConfig>
  );
}

export default App;
