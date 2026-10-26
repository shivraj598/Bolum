import { getVersion } from "@tauri-apps/api/app";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  api,
  levelPercent,
  settingValue,
  type HistoryEntry,
  type ModelInfo,
  type Phase,
  type Settings,
} from "./api";

const $ = <T extends HTMLElement>(selector: string) =>
  document.querySelector(selector) as T;

let settings: Settings = {};
let models: ModelInfo[] = [];

const IS_MAC = document.documentElement.classList.contains("mac");

/* ------------------------------------------------------------ navigation */

function showPage(page: string) {
  for (const button of document.querySelectorAll<HTMLButtonElement>("#nav button")) {
    const active = button.dataset.page === page;
    button.classList.toggle("active", active);
    if (active) $("#page-title").textContent = button.title;
  }
  for (const section of document.querySelectorAll<HTMLElement>("#content > section")) {
    section.hidden = section.dataset.page !== page;
  }
  $("#content").scrollTop = 0;
  try {
    localStorage.setItem("page", page);
  } catch {
    // Storage can be unavailable; the page simply isn't remembered.
  }
}

$("#nav").addEventListener("click", (event) => {
  const button = (event.target as HTMLElement).closest<HTMLButtonElement>("button[data-page]");
  if (button) showPage(button.dataset.page!);
});

$("#footer-model").addEventListener("click", () => showPage("models"));

// Like native toolbars, a hairline appears once content scrolls beneath it.
$("#content").addEventListener(
  "scroll",
  () => $("#page-title").parentElement!.classList.toggle("scrolled", $("#content").scrollTop > 0),
  { passive: true },
);

/* ------------------------------------------------ status, level, notices */

const levelBar = $("#level-bar");
let pendingLevel: number | null = null;

/** Level events arrive faster than the screen refreshes; draw once per frame. */
function showLevel(level: number) {
  if (pendingLevel === null) {
    requestAnimationFrame(() => {
      levelBar.style.width = `${levelPercent(pendingLevel ?? 0)}%`;
      pendingLevel = null;
    });
  }
  pendingLevel = level;
}

function setPhase(phase: Phase) {
  $("#status-dot").className = `status-dot ${phase}`;
  $("#status-label").textContent =
    phase === "listening" ? "Listening" : phase === "working" ? "Transcribing…" : "Ready";
  const dictate = $<HTMLButtonElement>("#dictate");
  dictate.textContent = phase === "listening" ? "Stop" : "Dictate";
  dictate.classList.toggle("recording", phase === "listening");
  dictate.disabled = phase === "working";
  if (phase === "idle") showLevel(0);
}

function notice(text: string) {
  const toast = document.createElement("div");
  toast.className = "toast";
  toast.textContent = text;
  $("#toasts").append(toast);
  setTimeout(() => toast.remove(), 6000);
}

void listen<Phase>("phase", (event) => {
  setPhase(event.payload);
  if (event.payload === "idle") void loadHistory();
});
void listen<number>("level", (event) => showLevel(event.payload));
void listen<string>("notice", (event) => notice(event.payload));
void listen<string>("transcript", (event) => {
  const last = $("#last-transcript");
  last.textContent = event.payload;
  last.classList.remove("muted");
  void loadHistory();
});
void listen<{ id: string; received: number; total: number }>("model-progress", (event) => {
  setProgress(event.payload.id, Math.round((event.payload.received / event.payload.total) * 100));
});

/* ----------------------------------------------------------- settings UI */

type Option = { value: string; label: string };

type Field = {
  key: string;
  label: string;
  type: "text" | "password" | "switch" | "select" | "segmented" | "number" | "textarea" | "shortcut";
  options?: () => Option[] | Promise<Option[]>;
  hint?: string;
};

type Group = { title: string; fields: Field[] };

const LANGUAGES: Option[] = [
  ["auto", "Auto-detect"],
  ["en", "English"],
  ["es", "Spanish"],
  ["fr", "French"],
  ["de", "German"],
  ["it", "Italian"],
  ["pt", "Portuguese"],
  ["nl", "Dutch"],
  ["pl", "Polish"],
  ["ru", "Russian"],
  ["uk", "Ukrainian"],
  ["tr", "Turkish"],
  ["ar", "Arabic"],
  ["hi", "Hindi"],
  ["ne", "Nepali"],
  ["zh", "Chinese"],
  ["ja", "Japanese"],
  ["ko", "Korean"],
  ["vi", "Vietnamese"],
  ["id", "Indonesian"],
  ["sv", "Swedish"],
  ["da", "Danish"],
  ["fi", "Finnish"],
  ["cs", "Czech"],
  ["el", "Greek"],
].map(([value, label]) => ({ value, label }));

const options = (...pairs: [string, string][]): (() => Option[]) => () =>
  pairs.map(([value, label]) => ({ value, label }));

const GENERAL_GROUPS: Group[] = [
  {
    title: "Shortcut",
    fields: [
      {
        key: "hotkey",
        label: "Dictation shortcut",
        type: "shortcut",
        hint: "Click and press a new combination. On Wayland/Hyprland, bind `bolum toggle` instead.",
      },
      {
        key: "push_to_talk",
        label: "Push to talk",
        type: "switch",
        hint: "Hold the shortcut while speaking instead of pressing it to start and stop.",
      },
    ],
  },
  {
    title: "Input",
    fields: [
      {
        key: "microphone",
        label: "Microphone",
        type: "select",
        options: async () => [
          { value: "", label: "System default" },
          ...(await api.getMicrophones()).map((name) => ({ value: name, label: name })),
        ],
      },
      {
        key: "language",
        label: "Language",
        type: "select",
        options: () => LANGUAGES,
        hint: "Auto-detect needs a multilingual model. English-only models ignore this.",
      },
    ],
  },
  {
    title: "Output",
    fields: [
      {
        key: "injection",
        label: "Text delivery",
        type: "select",
        options: options(["type", "Type it"], ["clipboard", "Paste from clipboard"], ["none", "Don't insert"]),
      },
      { key: "append_trailing_space", label: "Add a space after the text", type: "switch" },
      { key: "auto_submit", label: "Press Enter after inserting", type: "switch" },
    ],
  },
];

const ADVANCED_GROUPS: Group[] = [
  {
    title: "Appearance",
    fields: [
      {
        key: "theme",
        label: "Appearance",
        type: "segmented",
        options: options(["system", "Auto"], ["light", "Light"], ["dark", "Dark"]),
      },
      { key: "show_overlay", label: "Show overlay while dictating", type: "switch" },
      { key: "start_hidden", label: "Start hidden in the menu bar", type: "switch" },
    ],
  },
  {
    title: "Transcription",
    fields: [
      {
        key: "translate_to",
        label: "Translate to",
        type: "select",
        options: options(["", "Off"], ["en", "English"], ["de", "German"], ["es", "Spanish"], ["fr", "French"]),
        hint: "Whisper translates to English; Canary translates between en, de, es and fr.",
      },
      {
        key: "typing_tool",
        label: "Typing tool",
        type: "select",
        options: async () => [
          { value: "auto", label: "Automatic" },
          ...(await api.getTypingTools()).map((tool) => ({ value: tool, label: tool })),
        ],
        hint: "Used on Linux when text delivery is set to type.",
      },
    ],
  },
  {
    title: "History",
    fields: [{ key: "history_limit", label: "Entries to keep", type: "number" }],
  },
];

const POST_PROCESS_GROUPS: Group[] = [
  {
    title: "Endpoint",
    fields: [
      { key: "post_process.enabled", label: "Enable post-processing", type: "switch" },
      {
        key: "post_process.base_url",
        label: "Base URL",
        type: "text",
        hint: "Any OpenAI-compatible /chat/completions base, e.g. Ollama at http://localhost:11434/v1",
      },
      { key: "post_process.model", label: "Model", type: "text" },
      { key: "post_process.api_key", label: "API key", type: "password" },
    ],
  },
  {
    title: "Prompt",
    fields: [{ key: "post_process.prompt", label: "Instructions", type: "textarea" }],
  },
];

async function renderGroups(container: HTMLElement, groups: Group[]) {
  container.innerHTML = "";
  for (const group of groups) {
    const wrapper = document.createElement("div");
    wrapper.className = "group";
    wrapper.innerHTML = `<h2 class="group-title">${escapeHtml(group.title)}</h2>`;
    const body = document.createElement("div");
    body.className = "group-body";
    for (const field of group.fields) body.append(await renderField(field));
    wrapper.append(body);
    container.append(wrapper);
  }
}

async function renderField(field: Field): Promise<HTMLElement> {
  const row = document.createElement("div");
  row.className = field.type === "textarea" ? "setting stacked" : "setting";
  const text = document.createElement("div");
  text.className = "setting-text";
  text.innerHTML = `<span class="setting-label">${escapeHtml(field.label)}</span>${
    field.hint ? `<span class="setting-hint">${hintHtml(field.hint)}</span>` : ""
  }`;
  row.append(text);
  const current = settingValue(settings, field.key);

  if (field.type === "switch") {
    const control = document.createElement("label");
    control.className = "switch";
    const input = document.createElement("input");
    input.type = "checkbox";
    input.checked = Boolean(current);
    input.setAttribute("aria-label", field.label);
    input.onchange = () => void save(field.key, input.checked);
    control.append(input, document.createElement("span"));
    row.append(control);
  } else if (field.type === "segmented") {
    const control = document.createElement("div");
    control.className = "segmented";
    control.setAttribute("role", "radiogroup");
    control.setAttribute("aria-label", field.label);
    for (const choice of field.options ? await field.options() : []) {
      const button = document.createElement("button");
      button.type = "button";
      button.textContent = choice.label;
      button.setAttribute("role", "radio");
      const select = (on: boolean) => {
        button.classList.toggle("active", on);
        button.setAttribute("aria-checked", String(on));
      };
      select(String(current) === choice.value);
      button.onclick = () => {
        for (const other of control.children) other.classList.remove("active");
        select(true);
        void save(field.key, choice.value);
      };
      control.append(button);
    }
    row.append(control);
  } else if (field.type === "select") {
    const select = document.createElement("select");
    select.setAttribute("aria-label", field.label);
    const choices = field.options ? [...(await field.options())] : [];
    const currentValue = current == null ? "" : String(current);
    if (!choices.some((choice) => choice.value === currentValue)) {
      choices.unshift({ value: currentValue, label: currentValue || "System default" });
    }
    select.innerHTML = choices
      .map((choice) => `<option value="${escapeHtml(choice.value)}">${escapeHtml(choice.label)}</option>`)
      .join("");
    select.value = currentValue;
    select.onchange = () => void save(field.key, select.value);
    row.append(select);
  } else if (field.type === "shortcut") {
    row.append(shortcutControl(field.key, String(current ?? "")));
  } else if (field.type === "textarea") {
    const textarea = document.createElement("textarea");
    textarea.rows = 4;
    textarea.value = String(current ?? "");
    textarea.setAttribute("aria-label", field.label);
    textarea.onchange = () => void save(field.key, textarea.value);
    row.append(textarea);
  } else {
    const input = document.createElement("input");
    input.type = field.type;
    input.value = String(current ?? "");
    input.setAttribute("aria-label", field.label);
    input.onchange = () => {
      if (field.type !== "number") {
        void save(field.key, input.value);
        return;
      }
      // `Number("")` is 0, which would quietly discard the whole history.
      const parsed = Number(input.value);
      const value = Number.isFinite(parsed) ? Math.max(1, Math.round(parsed)) : 1;
      input.value = String(value);
      void save(field.key, value);
    };
    row.append(input);
  }
  return row;
}

async function save(key: string, value: unknown) {
  settings = await api.setSetting(key, value);
  if (key === "theme") applyTheme();
  if (key === "model") renderModels();
}

/** Applies the theme to the page and to the native window, so the macOS sidebar material matches. */
function applyTheme() {
  const theme = String(settingValue(settings, "theme") ?? "system");
  const forced = theme === "light" || theme === "dark" ? theme : null;
  if (forced) document.documentElement.dataset.theme = forced;
  else delete document.documentElement.dataset.theme;
  void getCurrentWindow().setTheme(forced).catch(() => {});
}

/* -------------------------------------------------------------- shortcut */

const MODIFIER_LABEL: Record<string, string> = IS_MAC
  ? { ctrl: "⌃", alt: "⌥", shift: "⇧", super: "⌘" }
  : { ctrl: "Ctrl", alt: "Alt", shift: "Shift", super: "Super" };

const MODIFIER_ALIAS: Record<string, string> = {
  control: "ctrl",
  option: "alt",
  cmd: "super",
  command: "super",
  meta: "super",
};

/** Splits an accelerator such as `ctrl+alt+space` into display keycaps. */
function keycaps(accelerator: string): string[] {
  return accelerator
    .split("+")
    .map((part) => part.trim().toLowerCase())
    .filter(Boolean)
    .map((part) => MODIFIER_ALIAS[part] ?? part)
    .map((part) =>
      MODIFIER_LABEL[part] ?? (part.length === 1 ? part.toUpperCase() : part[0].toUpperCase() + part.slice(1)),
    );
}

/** Maps a physical key to the accelerator name the global-shortcut plugin parses. */
function acceleratorKey(code: string): string | null {
  if (/^(Control|Alt|Shift|Meta|OS)(Left|Right)?$/.test(code)) return null;
  if (/^Key[A-Z]$/.test(code)) return code.slice(3).toLowerCase();
  if (/^Digit\d$/.test(code)) return code.slice(5);
  return code.toLowerCase();
}

function shortcutControl(key: string, current: string): HTMLElement {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "shortcut";
  button.setAttribute("aria-label", "Change shortcut");
  const show = (accelerator: string) => {
    button.innerHTML = accelerator
      ? keycaps(accelerator).map((cap) => `<kbd>${escapeHtml(cap)}</kbd>`).join("")
      : '<span class="muted">Not set</span>';
  };
  show(current);

  button.onclick = () => {
    if (button.classList.contains("recording")) return;
    button.classList.add("recording");
    button.textContent = "Press a shortcut…";

    const onKey = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopPropagation();
      const modifiers = [
        event.ctrlKey && "ctrl",
        event.altKey && "alt",
        event.shiftKey && "shift",
        event.metaKey && "super",
      ].filter(Boolean) as string[];
      if (event.code === "Escape" && modifiers.length === 0) {
        finish(null);
        return;
      }
      const name = acceleratorKey(event.code);
      if (!name) return;
      // Shift alone would swallow ordinary capitals system-wide.
      const chorded = modifiers.some((modifier) => modifier !== "shift");
      if (!chorded && !/^f\d+$/.test(name)) {
        button.textContent = IS_MAC ? "Include ⌃, ⌥ or ⌘…" : "Include Ctrl, Alt or Super…";
        return;
      }
      finish([...modifiers, name].join("+"));
    };
    const onBlur = () => finish(null);
    const finish = (accelerator: string | null) => {
      window.removeEventListener("keydown", onKey, true);
      button.removeEventListener("blur", onBlur);
      button.classList.remove("recording");
      if (accelerator) {
        show(accelerator);
        void save(key, accelerator);
      } else {
        show(String(settingValue(settings, key) ?? ""));
      }
    };
    window.addEventListener("keydown", onKey, true);
    button.addEventListener("blur", onBlur);
  };
  return button;
}

/* ---------------------------------------------------------------- models */

const ENGINE_LABEL: Record<string, string> = {
  whisper: "Whisper",
  parakeet: "Parakeet",
  canary: "Canary",
};

const TRASH_ICON =
  '<svg viewBox="0 0 24 24"><path d="M4 7h16M10 11v6M14 11v6M6 7l1 12a2 2 0 0 0 2 2h6a2 2 0 0 0 2-2l1-12M9 7V4h6v3" /></svg>';

let modelTimer: number | undefined;
let modelsSignature = "";

const selectedModel = () => String(settingValue(settings, "model") ?? "");

async function loadModels() {
  models = await api.listModels();
  renderModels();
  // Progress arrives as events; this slow poll only catches downloads that
  // started before the window opened, and notices when they settle.
  if (models.some((model) => model.downloading)) {
    modelTimer ??= window.setInterval(() => void loadModels(), 1500);
  } else if (modelTimer !== undefined) {
    window.clearInterval(modelTimer);
    modelTimer = undefined;
  }
}

/** Rebuilds the cards only when what they show changes; progress is patched in place. */
function renderModels() {
  const selected = selectedModel();
  const signature = JSON.stringify([
    selected,
    models.map((model) => [model.id, model.downloaded, model.downloading, model.progress > 0]),
  ]);
  if (signature !== modelsSignature) {
    modelsSignature = signature;
    $("#models-recommended").innerHTML = models
      .filter((model) => model.recommended)
      .map((model) => modelCard(model, selected))
      .join("");
    $("#models-more").innerHTML = models
      .filter((model) => !model.recommended)
      .map((model) => modelCard(model, selected))
      .join("");
  } else {
    for (const model of models) if (model.downloading) setProgress(model.id, model.progress);
  }
  $("#welcome").hidden = models.some((model) => model.downloaded);

  const active = models.find((model) => model.id === selected && model.downloaded);
  $("#footer-model-name").textContent = active ? active.name : "No model — choose one";
  $("#footer-model").classList.toggle("warning", !active);
}

function setProgress(id: string, percent: number) {
  const card = document.querySelector<HTMLElement>(`[data-model="${CSS.escape(id)}"]`);
  if (!card) return;
  const bar = card.querySelector<HTMLElement>(".progress");
  const label = card.querySelector<HTMLElement>(".progress-label");
  if (bar) bar.style.width = `${percent}%`;
  if (label) label.textContent = `${percent}%`;
}

function sizeLabel(megabytes: number) {
  return megabytes >= 1000 ? `${(megabytes / 1000).toFixed(1)} GB` : `${megabytes} MB`;
}

function score(label: string, value: number) {
  return `<span class="score"><span>${label}</span><span class="score-bar"><i style="width:${Math.round(
    value * 100,
  )}%"></i></span></span>`;
}

function modelCard(model: ModelInfo, selected: string) {
  const active = model.downloaded && model.id === selected;
  let actions: string;
  if (active) {
    actions = `<span class="chip accent">Active</span>
      <button class="icon-button" data-action="delete" title="Delete model">${TRASH_ICON}</button>`;
  } else if (model.downloaded) {
    actions = `<button class="button" data-action="use">Use</button>
      <button class="icon-button" data-action="delete" title="Delete model">${TRASH_ICON}</button>`;
  } else if (model.downloading) {
    actions = `<button class="button" data-action="cancel">Cancel</button>`;
  } else {
    actions = `<button class="button primary" data-action="download">${model.progress > 0 ? "Resume" : "Download"}</button>`;
  }
  const progress = model.downloading
    ? `<div class="model-progress"><div class="bar"><i class="progress" style="width:${model.progress}%"></i></div>
        <span class="progress-label">${model.progress}%</span></div>`
    : "";
  return `<article class="model-card${active ? " selected" : ""}" data-model="${escapeHtml(model.id)}">
    <div class="model-main">
      <div class="model-title">
        <h3>${escapeHtml(model.name)}</h3>
        ${model.recommended ? '<span class="chip soft">Recommended</span>' : ""}
      </div>
      <p class="model-desc">${escapeHtml(model.description)}</p>
      <div class="model-meta">
        ${score("Accuracy", model.accuracy)}
        ${score("Speed", model.speed)}
        <span class="tags">
          <span class="chip">${escapeHtml(ENGINE_LABEL[model.engine] ?? model.engine)}</span>
          <span class="chip">${escapeHtml(model.languages)}</span>
          <span class="chip">${sizeLabel(model.size_mb)}</span>
        </span>
      </div>
      ${progress}
    </div>
    <div class="model-actions">${actions}</div>
  </article>`;
}

function downloadModel(model: ModelInfo) {
  // The download runs to completion in the backend; the card stays fresh
  // through progress events and the polling refresh.
  api
    .downloadModel(model.id)
    .then(async () => {
      await loadModels();
      const current = models.find((other) => other.id === selectedModel());
      if (!current?.downloaded) {
        await save("model", model.id);
        notice(`${model.name} is ready and in use.`);
      } else {
        notice(`${model.name} downloaded.`);
      }
    })
    .catch((error) => {
      if (!String(error).includes("cancelled")) notice(String(error));
      void loadModels();
    });
}

/** Deleting asks once: the first click arms the button for a few seconds. */
function confirmDelete(button: HTMLButtonElement) {
  if (button.classList.contains("armed")) return true;
  button.classList.add("armed");
  button.title = "Click again to delete";
  setTimeout(() => {
    button.classList.remove("armed");
    button.title = "Delete model";
  }, 3000);
  return false;
}

$('section[data-page="models"]').addEventListener("click", async (event) => {
  const button = (event.target as HTMLElement).closest<HTMLButtonElement>("button[data-action]");
  const card = (event.target as HTMLElement).closest<HTMLElement>("[data-model]");
  if (!button || !card) return;
  const model = models.find((other) => other.id === card.dataset.model);
  if (!model) return;
  try {
    switch (button.dataset.action) {
      case "download":
        downloadModel(model);
        break;
      case "cancel":
        await api.cancelDownload(model.id);
        break;
      case "delete":
        if (!confirmDelete(button)) return;
        await api.deleteModel(model.id);
        break;
      case "use":
        await save("model", model.id);
        break;
    }
  } catch (error) {
    notice(String(error));
  }
  await loadModels();
});

/* --------------------------------------------------------------- history */

const COPY_ICON =
  '<svg viewBox="0 0 24 24"><rect x="9" y="9" width="11" height="11" rx="2" /><path d="M5 15V6a2 2 0 0 1 2-2h8" /></svg>';

async function loadHistory() {
  const entries = await api.listHistory();
  $("#history-list").innerHTML = entries.length
    ? entries.map(historyRow).join("")
    : '<div class="empty">No dictations yet. Press the shortcut and start speaking.</div>';
}

function historyRow(entry: HistoryEntry) {
  const date = new Date(entry.timestamp_ms);
  const today = date.toDateString() === new Date().toDateString();
  const when = today
    ? `Today ${date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}`
    : date.toLocaleString([], { dateStyle: "medium", timeStyle: "short" });
  const text = entry.post_processed ?? entry.text;
  return `<article class="history-item" data-history="${entry.id}">
    <p class="history-text">${escapeHtml(text)}</p>
    <div class="history-meta">
      <span>${escapeHtml(when)} · ${escapeHtml(entry.model)} · ${(entry.duration_ms / 1000).toFixed(1)} s</span>
      <span class="history-actions">
        <button class="icon-button" data-action="copy" title="Copy">${COPY_ICON}</button>
        <button class="icon-button" data-action="delete" title="Delete">${TRASH_ICON}</button>
      </span>
    </div>
  </article>`;
}

$("#history-list").addEventListener("click", async (event) => {
  const button = (event.target as HTMLElement).closest<HTMLButtonElement>("button[data-action]");
  const row = (event.target as HTMLElement).closest<HTMLElement>("[data-history]");
  if (!button || !row) return;
  const id = Number(row.dataset.history);
  try {
    if (button.dataset.action === "copy") {
      await api.copyHistory(id);
      notice("Copied to the clipboard.");
    }
    if (button.dataset.action === "delete") await api.deleteHistory(id);
  } catch (error) {
    notice(String(error));
  }
  await loadHistory();
});

/* ------------------------------------------------------------------ misc */

$("#dictate").addEventListener("click", async () => {
  try {
    if ((await api.getStatus()) === "listening") await api.stopRecording();
    else await api.startRecording();
  } catch (error) {
    notice(String(error));
  }
});

function escapeHtml(value: string) {
  return value.replace(
    /[&<>"']/g,
    (character) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[character]!,
  );
}

/** Escapes a hint and renders `backticked` spans as code. */
function hintHtml(value: string) {
  return escapeHtml(value).replace(/`([^`]+)`/g, "<code>$1</code>");
}

async function boot() {
  settings = await api.getSettings();
  applyTheme();
  setPhase(await api.getStatus());
  await renderGroups($("#general-groups"), GENERAL_GROUPS);
  await renderGroups($("#advanced-groups"), ADVANCED_GROUPS);
  await renderGroups($("#post-process-groups"), POST_PROCESS_GROUPS);
  await loadModels();
  await loadHistory();
  $("#version").textContent = await getVersion().catch(() => "unknown");

  let page = "general";
  try {
    const stored = localStorage.getItem("page");
    if (stored && document.querySelector(`#content > section[data-page="${CSS.escape(stored)}"]`)) {
      page = stored;
    }
  } catch {
    // Fall back to the first page.
  }
  // New users land on Models: nothing works until one is downloaded.
  showPage(models.some((model) => model.downloaded) ? page : "models");
}

void boot();
