# Bolum

Push-to-talk dictation for macOS and Linux desktops. Press a shortcut (or run `bolum toggle`
from a compositor keybinding), speak, and the transcript is typed into whatever window has
focus.
Everything runs locally: Whisper through whisper.cpp, or NVIDIA Parakeet/Canary through
ONNX Runtime.

Built in the spirit of [Handy](https://github.com/cjpais/Handy): a Tauri shell, a small
Rust core, and a thin web UI, with the same download-and-verify model strategy.

## Features

- **Local engines** — Whisper (ggml, via whisper.cpp) and NVIDIA Parakeet TDT / Canary
  (ONNX, int8) with SHA-256 verified downloads.
- **Two ways to trigger** — an in-app global shortcut where the desktop allows it, plus a
  `bolum toggle|start|stop|cancel|status|quit` control socket for compositor keybindings.
- **Text delivery that adapts** — on macOS, native key events (CGEvent) or a clipboard
  paste; on Linux, types with `wtype`, `kwtype`, `dotool`, `ydotool` or `xdotool`, or
  pastes through the clipboard.
- **Apple Silicon acceleration** — Whisper runs on the GPU through Metal on macOS.
- **Optional LLM cleanup** — send the transcript through any OpenAI-compatible endpoint
  (Ollama, llama.cpp, LM Studio, OpenRouter, …) before it is inserted.
- **History** — every dictation is kept in a JSON-lines file with copy and delete from
  the app.
- **Tray + overlay feedback** — the tray icon and a small overlay show listening and
  transcribing states.

## Requirements

### macOS

- macOS 11 or later (Apple Silicon or Intel)
- Rust (stable), Node.js 20+ and `cmake` (`brew install rust node cmake`)
- Xcode Command Line Tools (`xcode-select --install`)

On first launch macOS asks for **Microphone** and **Accessibility** access (System
Settings → Privacy & Security → Accessibility); the latter lets Bolum insert text into
other apps. Bolum lives in the menu bar and has no Dock icon.

### Linux

- Wayland or X11 session
- Rust (stable) and Node.js 20+
- `cmake` (whisper.cpp build), plus GTK/WebKitGTK development packages for Tauri
- One typing tool on `PATH`: `wtype` (Wayland), `xdotool`, `ydotool`, `dotool` or `kwtype`
- One clipboard tool: `wl-copy`, `xclip` or `xsel`

On Arch:

```sh
sudo pacman -S base-devel cmake webkit2gtk-4.1 gtk3 libappindicator-gtk3 librsvg wtype wl-clipboard
```

## Build and run

```sh
npm install
npm run tauri build      # release bundles in src-tauri/target/release/bundle
                         #   macOS: Bolum.app + .dmg   Linux: AppImage + .deb
npm run tauri dev        # development
```

## Usage

Start Bolum (it stays in the tray; the settings window opens on first run):

```sh
bolum            # the GUI/tray process
bolum toggle     # ask the running instance to start/stop dictation
bolum status     # idle | listening | working
```

On macOS the default shortcut is **⌃⌥Space** (Control-Option-Space); change it in the
Dictation tab. To use the CLI, link the binary onto your `PATH`:

```sh
ln -s /Applications/Bolum.app/Contents/MacOS/bolum /usr/local/bin/bolum
```

Hyprland keybinding:

```ini
exec-once = bolum
bind = SUPER, D, exec, bolum toggle
```

Open the **Models** tab, download a model, and press **use**. Then dictate with the
**Dictate** button, the configured shortcut, or `bolum toggle`.

## Models

| Model | Engine | Size | Languages |
| --- | --- | --- | --- |
| Whisper Base (English) | whisper.cpp | ~60 MB | English |
| Whisper Small | whisper.cpp | ~190 MB | 99 languages |
| Whisper Large v3 Turbo | whisper.cpp | ~574 MB | 99 languages |
| NVIDIA Parakeet TDT 0.6B v3 | ONNX int8 | ~670 MB | 25 European languages |
| NVIDIA Canary 180M Flash | ONNX int8 | ~213 MB | en, de, es, fr (+ translation) |

Weights are fetched from Hugging Face and verified against the SHA-256 recorded in
[`src-tauri/src/models.rs`](src-tauri/src/models.rs) before use. Parakeet and Canary use
the ONNX conversions published by [istupakov](https://huggingface.co/istupakov) in the
[`onnx-asr`](https://github.com/istupakov/onnx-asr) project, built on NVIDIA's
[Parakeet](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3) and
[Canary](https://huggingface.co/nvidia/canary-180m-flash) checkpoints (CC-BY-4.0).

## How it works

```
shortcut / `bolum toggle`
        │
        ▼
  cpal capture ──► 16 kHz mono ──► engine ──► (LLM cleanup) ──► typing tool
   (audio.rs)                    (engines/)    (postprocess.rs)   (inject.rs)
```

- `src-tauri/src/settings.rs` — JSON settings store
- `src-tauri/src/audio.rs` — microphone capture, resampling, WAV writing
- `src-tauri/src/models.rs` — catalog, verified downloads, progress events
- `src-tauri/src/engines/` — Whisper (whisper-rs) and NeMo ONNX (ort) transcription
- `src-tauri/src/pipeline.rs` — the dictation state machine
- `src-tauri/src/ipc.rs` — the control socket
- `src/` — the settings window and overlay (vanilla TypeScript + Vite)

## Credits

- [Handy](https://github.com/cjpais/Handy) (MIT) for the product shape and the
  model-download strategy this project follows.
- [whisper.cpp](https://github.com/ggml-org/whisper.cpp) and
  [whisper-rs](https://github.com/tazz4843/whisper-rs) for local Whisper inference.
- [onnx-asr](https://github.com/istupakov/onnx-asr) for the NVIDIA ONNX export contracts.
- NVIDIA for the Parakeet and Canary model checkpoints.

## License

MIT — see [LICENSE](LICENSE).
