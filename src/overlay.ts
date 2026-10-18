import { listen } from "@tauri-apps/api/event";

import { levelPercent } from "./api";

const label = document.querySelector("#overlay-label") as HTMLElement;
const bar = document.querySelector("#overlay-level i") as HTMLElement;

void listen<string>("phase", (e) => {
  label.textContent = e.payload === "listening" ? "Listening" : "Transcribing";
  if (e.payload === "idle") bar.style.width = "0%";
});

void listen<number>("level", (e) => {
  bar.style.width = `${levelPercent(e.payload)}%`;
});
