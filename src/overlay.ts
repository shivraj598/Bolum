import { listen } from "@tauri-apps/api/event";

import { api, levelPercent, type Phase } from "./api";

const pill = document.querySelector("#overlay") as HTMLElement;
const bars = [...document.querySelectorAll<HTMLElement>(".ov-wave i")];

// The newest level enters on the right, so the bars scroll like a waveform.
const levels = bars.map(() => 0);
let frame = 0;

/** Level events can outpace the display; draw at most once per frame. */
function render() {
  if (frame) return;
  frame = requestAnimationFrame(() => {
    frame = 0;
    bars.forEach((bar, index) => {
      const height = Math.max(3, Math.min(18, 3 + Math.pow(levels[index], 0.7) * 15));
      bar.style.height = `${height}px`;
    });
  });
}

void listen<Phase>("phase", (event) => {
  pill.classList.toggle("working", event.payload === "working");
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

render();
