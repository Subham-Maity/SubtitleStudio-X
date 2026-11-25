# Changelog

All notable changes to **SubtitleStudio X** are documented in this file.

## [0.1.0] - 2026-11-25

### What's New in v0.1.0 (Initial Native Release)
- **Complete Rust Core Rewrite:** Replaced the multi-process Node/NestJS/Redis/Postgres/Python stack with a single native Tauri 2 desktop binary backed by `studio_core` and `subtitle_render` plus embedded SQLite (`WAL` mode).
- **Single Deterministic Renderer (`subtitle_render`):** Powered by `skia-safe` + HarfBuzz text shaping (`AlphaType::Unpremul` RGBA8888) for identical pixel parity (`diff == 0`) between live canvas preview (`requestVideoFrameCallback`) and exported MP4/MOV video.
- **Full Animation, Effect & Multilingual Engine:**
  - 9 entrance animations (`none`, `fade`, `pop`, `slide-up`, `zoom`, `typewriter`, `word-reveal`, `bounce`, `blur-in`) and 2 exits (`fade-out`, `slide-down`) with closed-form `linear`, `cubic`, `back`, `elastic`, and `spring` easings.
  - Index-accurate karaoke modes (`highlight-current-word`, `progressive-fill`, `pop-current`) that avoid text-collision bugs on repeated words.
  - True clip/mask `inside`/`outside`/`center` strokes, Gaussian shadow/glow, linear gradients, rounded background boxes, rotation, per-cue colors, and uppercase transforms.
  - Cross-platform shaping for English, Hindi (Devanagari & Roman Hinglish), Urdu/RTL, and color emoji plus `notify` live custom font hot-reload.
- **Offline Whisper GPU + CPU Fallback (`whisper-rs`):**
  - Vulkan GPU acceleration on Windows/Linux (delay-loaded on Windows so GPU-less PCs launch safely and fall back to CPU), Metal GPU support on macOS, and explicit VRAM reclaim before export.
  - On-demand GGML model downloads (`large-v3-turbo`, `large-v3-turbo-q5_0`, `small`, `base`) with streaming progress UI.
- **Direct FFmpeg Pipeline & Multi-GPU Detection:**
  - 1-frame hardware encoder auto-detection (`h264_nvenc` / `hevc_nvenc`, `h264_amf` / `hevc_amf`, `h264_qsv` / `hevc_qsv`, `h264_videotoolbox` / `hevc_videotoolbox`, CPU `libx264` / `libx265` fallback).
  - Bounded-channel pipelined RGBA frame streaming, Captions-Only mode, SRT/TXT export & batch timeline merge, and folder batch translate (`index.json` resume).
- **UI & Packaging:** Rebranded to **SubtitleStudio X** with dark theme, floating toolbar, interactive DOM handles, batch queue telemetry, author attribution links in the navbar, MIT license, and multi-OS CI/Release GitHub Actions workflows.

