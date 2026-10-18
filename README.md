# Bolum

Push-to-talk dictation for Linux desktops: press a shortcut, speak, and the transcript is
typed into the focused window. Transcription runs locally with Whisper (whisper.cpp) or
NVIDIA Parakeet/Canary (ONNX Runtime).

Built in the spirit of [Handy](https://github.com/cjpais/Handy): a Tauri v2 shell, a small
Rust core and a thin web UI.

## Requirements

- Linux (Wayland or X11), Rust stable, Node.js 20+, `cmake`
- Tauri's GTK/WebKitGTK development packages
- A typing tool (`wtype`, `xdotool`, `ydotool`, `dotool` or `kwtype`) and a clipboard tool
  (`wl-copy`, `xclip` or `xsel`)

## Build and run

```sh
npm install
npm run tauri dev        # development
npm run tauri build      # AppImage + deb bundles
```

## License

MIT — see [LICENSE](LICENSE).
