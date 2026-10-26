// Builds every icon Bolum ships from the sources in `branding/`.
//
//   npm run icons
//
// - branding/app-icon.svg (or .png, 1024×1024) → src-tauri/icons/* for the
//   Dock, Finder, the .dmg and the Linux packages.
// - branding/tray/<state>.svg → branding/tray/<state>.png for the menu bar.
//   A state with only a hand-made PNG is left as it is.

import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const brand = join(root, "branding");
const icons = join(root, "src-tauri", "icons");
const TRAY_STATES = ["idle-template", "idle", "recording", "transcribing"];
const TRAY_PX = 44; // 22 pt at 2×, the macOS menu-bar size

function tauriIcon(...args) {
  execFileSync("npx", ["tauri", "icon", ...args], { cwd: root, stdio: ["ignore", "ignore", "inherit"] });
}

const appIcon = ["app-icon.svg", "app-icon.png"].map((name) => join(brand, name)).find(existsSync);
if (!appIcon) {
  console.error("branding/app-icon.svg or branding/app-icon.png is missing");
  process.exit(1);
}
tauriIcon(appIcon, "-o", icons);
// Bolum is desktop-only; `tauri icon` also emits mobile sets.
for (const mobile of ["android", "ios"]) rmSync(join(icons, mobile), { recursive: true, force: true });
console.log(`app icon   ${appIcon.slice(root.length)} → src-tauri/icons/`);

for (const state of TRAY_STATES) {
  const svg = join(brand, "tray", `${state}.svg`);
  if (!existsSync(svg)) continue;
  const scratch = mkdtempSync(join(tmpdir(), "bolum-tray-"));
  try {
    tauriIcon(svg, "-o", scratch, "-p", String(TRAY_PX));
    copyFileSync(join(scratch, `${TRAY_PX}x${TRAY_PX}.png`), join(brand, "tray", `${state}.png`));
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
  console.log(`menu bar   branding/tray/${state}.svg → ${state}.png`);
}
