import { listen } from "@tauri-apps/api/event";

import { api, levelPercent, settingValue, type Phase } from "./api";

const pill = document.querySelector("#overlay") as HTMLElement;
const bars = [...document.querySelectorAll<HTMLElement>(".ov-wave i")];

// The newest level enters on the right, so the bars scroll like a waveform.
const levels = bars.map(() => 0);

function render() {
  bars.forEach((bar, index) => {
    const height = Math.max(3, Math.min(18, 3 + Math.pow(levels[index], 0.7) * 15));
    bar.style.height = `${height}px`;
  });
}

/** Follows the theme chosen in the main window; this window lives as long as the app. */
async function applyTheme() {
  const theme = String(settingValue(await api.getSettings(), "theme") ?? "system");
  if (theme === "light" || theme === "dark") document.documentElement.dataset.theme = theme;
  else delete document.documentElement.dataset.theme;
}

void listen<Phase>("phase", (event) => {
  pill.classList.toggle("working", event.payload === "working");
  if (event.payload === "listening") void applyTheme();
  if (event.payload !== "working") {
    levels.fill(0);
    render();
  }
});

void listen<number>("level", (event) => {
  levels.shift();
  levels.push(levelPercent(event.payload) / 100);
  render();
});

document.querySelector("#overlay-cancel")!.addEventListener("click", () => {
  void api.cancelRecording();
});

void applyTheme();
render();
