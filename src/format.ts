/**
 * Pure formatting and parsing helpers for the settings window. They touch
 * neither the DOM nor Tauri, so the unit tests run them in plain Node.
 */

const MAC_MODIFIERS: Record<string, string> = { ctrl: "⌃", alt: "⌥", shift: "⇧", super: "⌘" };
const PC_MODIFIERS: Record<string, string> = { ctrl: "Ctrl", alt: "Alt", shift: "Shift", super: "Super" };

const MODIFIER_ALIAS: Record<string, string> = {
  control: "ctrl",
  option: "alt",
  cmd: "super",
  command: "super",
  meta: "super",
};

/** Splits an accelerator such as `ctrl+alt+space` into display keycaps. */
export function keycaps(accelerator: string, mac: boolean): string[] {
  const labels = mac ? MAC_MODIFIERS : PC_MODIFIERS;
  return accelerator
    .split("+")
    .map((part) => part.trim().toLowerCase())
    .filter(Boolean)
    .map((part) => MODIFIER_ALIAS[part] ?? part)
    .map((part) => labels[part] ?? (part.length === 1 ? part.toUpperCase() : part[0].toUpperCase() + part.slice(1)));
}

/** Maps a physical key to the accelerator name the global-shortcut plugin parses. */
export function acceleratorKey(code: string): string | null {
  if (/^(Control|Alt|Shift|Meta|OS)(Left|Right)?$/.test(code)) return null;
  if (/^Key[A-Z]$/.test(code)) return code.slice(3).toLowerCase();
  if (/^Digit\d$/.test(code)) return code.slice(5);
  return code.toLowerCase();
}

/**
 * True when `modifiers` + `key` is safe to register system-wide. Function keys
 * stand alone; anything else needs Ctrl, Alt or Super, because Shift alone
 * would swallow ordinary capitals.
 */
export function isUsableShortcut(modifiers: string[], key: string): boolean {
  return /^f\d+$/.test(key) || modifiers.some((modifier) => modifier !== "shift");
}

export function escapeHtml(value: string): string {
  return value.replace(
    /[&<>"']/g,
    (character) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[character]!,
  );
}

/** Escapes a hint and renders `backticked` spans as code. */
export function hintHtml(value: string): string {
  return escapeHtml(value).replace(/`([^`]+)`/g, "<code>$1</code>");
}

export function sizeLabel(megabytes: number): string {
  return megabytes >= 1000 ? `${(megabytes / 1000).toFixed(1)} GB` : `${megabytes} MB`;
}

/** Maps a 0..1 score to the filled steps of a five-step meter, never fewer than one. */
export function scoreSteps(value: number): number {
  return Math.min(5, Math.max(1, Math.round(value * 5)));
}
