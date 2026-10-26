import { listen } from "@tauri-apps/api/event";
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

/* ------------------------------------------------------------------ tabs */

$("#tabs").addEventListener("click", (event) => {
  const button = (event.target as HTMLElement).closest<HTMLButtonElement>("button[data-tab]");
  if (!button) return;
  for (const other of document.querySelectorAll("#tabs button")) {
    other.classList.toggle("active", other === button);
  }
  for (const section of document.querySelectorAll<HTMLElement>("main > section")) {
    section.hidden = section.id !== `tab-${button.dataset.tab}`;
  }
});

/* ------------------------------------------------ status, level, notices */

const pill = $("#status-pill");
const levelBar = $("#level-bar");

function setPhase(phase: Phase) {
  pill.textContent = phase === "listening" ? "listening" : phase === "working" ? "working…" : "idle";
  pill.className = `pill ${phase}`;
  if (phase === "idle") levelBar.style.width = "0%";
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
void listen<number>("level", (event) => {
  levelBar.style.width = `${levelPercent(event.payload)}%`;
});
void listen<string>("notice", (event) => notice(event.payload));
void listen<string>("transcript", (event) => {
  $("#last-transcript").textContent = event.payload;
  void loadHistory();
});
void listen<{ id: string; received: number; total: number }>("model-progress", (event) => {
  const row = document.querySelector<HTMLElement>(
    `[data-model="${CSS.escape(event.payload.id)}"] .progress`,
  );
  if (row) row.style.width = `${Math.round((event.payload.received / event.payload.total) * 100)}%`;
});

/* ----------------------------------------------------------- settings UI */

type Field = {
  key: string;
  label: string;
  type: "text" | "password" | "bool" | "select" | "number" | "textarea";
  options?: () => Promise<string[]>;
  hint?: string;
};

const DICTATION_FIELDS: Field[] = [
  {
    key: "hotkey",
    label: "Trigger shortcut",
    type: "text",
    hint: "Registered when the desktop allows it (always on macOS). On Wayland/Hyprland, bind `bolum toggle` instead.",
  },
  { key: "push_to_talk", label: "Push to talk (hold the shortcut)", type: "bool" },
  {
    key: "microphone",
    label: "Microphone",
    type: "select",
    options: async () => ["", ...(await api.getMicrophones())],
  },
  {
    key: "model",
    label: "Model",
    type: "select",
    options: async () =>
      (await api.listModels()).filter((model) => model.downloaded).map((model) => model.id),
  },
  {
    key: "language",
    label: "Language",
    type: "text",
    hint: "`auto` to detect, or a code such as en, de, es, fr, ru.",
  },
  {
    key: "translate_to",
    label: "Translate to",
    type: "text",
    hint: "Optional: `en` with Whisper, or en/de/es/fr with Canary.",
  },
  {
    key: "injection",
    label: "Text delivery",
    type: "select",
    options: async () => ["type", "clipboard", "none"],
  },
  {
    key: "typing_tool",
    label: "Typing tool",
    type: "select",
    options: async () => ["auto", ...(await api.getTypingTools())],
  },
  { key: "append_trailing_space", label: "Append a trailing space", type: "bool" },
  { key: "auto_submit", label: "Press Enter after inserting", type: "bool" },
  { key: "show_overlay", label: "Show overlay while dictating", type: "bool" },
  { key: "start_hidden", label: "Start hidden in the tray", type: "bool" },
  { key: "history_limit", label: "History entries to keep", type: "number" },
];

const POST_PROCESS_FIELDS: Field[] = [
  { key: "post_process.enabled", label: "Enable LLM post-processing", type: "bool" },
  {
    key: "post_process.base_url",
    label: "Endpoint",
    type: "text",
    hint: "Any OpenAI-compatible /chat/completions base URL, e.g. http://localhost:11434/v1",
  },
  { key: "post_process.model", label: "Model", type: "text" },
  { key: "post_process.api_key", label: "API key", type: "password" },
  { key: "post_process.prompt", label: "Prompt", type: "textarea" },
];

async function renderFields(container: HTMLElement, fields: Field[]) {
  container.innerHTML = "";
  for (const field of fields) {
    const row = document.createElement("label");
    row.className = "row";
    const name = document.createElement("span");
    name.textContent = field.label;
    if (field.hint) {
      const hint = document.createElement("small");
      hint.className = "hint";
      hint.textContent = field.hint;
      name.append(document.createElement("br"), hint);
    }
    row.append(name);
    const current = settingValue(settings, field.key);

    if (field.type === "bool") {
      const input = document.createElement("input");
      input.type = "checkbox";
      input.checked = Boolean(current);
      input.onchange = () => void save(field.key, input.checked);
      row.append(input);
    } else if (field.type === "select") {
      const select = document.createElement("select");
      const options = field.options ? await field.options() : [];
      const currentOption = current == null ? "" : String(current);
      if (!options.includes(currentOption)) options.unshift(currentOption);
      select.innerHTML = options
        .map((option) => `<option value="${escapeHtml(option)}">${escapeHtml(option || "system default")}</option>`)
        .join("");
      select.value = currentOption;
      select.onchange = () => void save(field.key, select.value);
      row.append(select);
    } else if (field.type === "textarea") {
      const textarea = document.createElement("textarea");
      textarea.rows = 3;
      textarea.value = String(current ?? "");
      textarea.onchange = () => void save(field.key, textarea.value);
      row.append(textarea);
    } else {
      const input = document.createElement("input");
      input.type = field.type === "number" ? "number" : field.type === "password" ? "password" : "text";
      input.value = String(current ?? "");
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
    container.append(row);
  }
}

async function save(key: string, value: unknown) {
  settings = await api.setSetting(key, value);
}

/* ---------------------------------------------------------------- models */

const ENGINE_LABEL: Record<string, string> = {
  whisper: "Whisper (whisper.cpp)",
  parakeet: "NVIDIA Parakeet TDT",
  canary: "NVIDIA Canary",
};

let modelTimer: number | undefined;

async function loadModels() {
  const models = await api.listModels();
  const selected = String(settingValue(settings, "model") ?? "");
  $("#model-list").innerHTML = models.map((model) => modelRow(model, selected)).join("");
  if (models.some((model) => model.downloading)) {
    scheduleModelsRefresh();
  } else if (modelTimer !== undefined) {
    window.clearInterval(modelTimer);
    modelTimer = undefined;
  }
}

/// Downloads report progress from the backend; keep the rows fresh until they settle.
function scheduleModelsRefresh() {
  if (modelTimer !== undefined) return;
  modelTimer = window.setInterval(() => void loadModels(), 500);
}

function modelRow(model: ModelInfo, selected: string) {
  const state = model.downloaded
    ? model.id === selected ? "in use" : "ready"
    : model.downloading ? "downloading…" : `${model.size_mb} MB`;
  const actions = model.downloaded
    ? `<button data-action="use">use</button> <button data-action="delete">delete</button>`
    : model.downloading
      ? `<button data-action="cancel">cancel</button>`
      : `<button data-action="download">download</button>`;
  return `<li data-model="${escapeHtml(model.id)}" class="${model.id === selected ? "selected" : ""}">
    <div><strong>${escapeHtml(model.name)}</strong>
      <span class="hint">· ${escapeHtml(ENGINE_LABEL[model.engine] ?? model.engine)} · ${escapeHtml(model.languages)}</span></div>
    <div class="hint">${escapeHtml(model.description)}</div>
    <div class="row"><span class="hint">${state}</span><span>${actions}</span></div>
    <div class="bar"><i class="progress" style="width:${model.progress}%"></i></div>
  </li>`;
}

$("#model-list").addEventListener("click", async (event) => {
  const button = (event.target as HTMLElement).closest<HTMLButtonElement>("button[data-action]");
  const row = (event.target as HTMLElement).closest<HTMLElement>("[data-model]");
  if (!button || !row) return;
  const id = row.dataset.model!;
  const action = button.dataset.action;
  try {
    if (action === "download") {
      // The download runs to completion in the backend; the row stays fresh
      // through progress events and the polling refresh.
      void api.downloadModel(id).catch((error) => notice(String(error)));
    } else if (action === "cancel") {
      await api.cancelDownload(id);
    } else if (action === "delete") {
      await api.deleteModel(id);
    } else if (action === "use") {
      await save("model", id);
    }
  } catch (error) {
    notice(String(error));
  }
  await loadModels();
});

/* --------------------------------------------------------------- history */

async function loadHistory() {
  const entries = await api.listHistory();
  $("#history-list").innerHTML = entries.length
    ? entries.map(historyRow).join("")
    : '<li class="hint">No dictations yet.</li>';
}

function historyRow(entry: HistoryEntry) {
  const when = new Date(entry.timestamp_ms).toLocaleString();
  const text = entry.post_processed ?? entry.text;
  return `<li data-history="${entry.id}">
    <div>${escapeHtml(text)}</div>
    <div class="hint">${escapeHtml(when)} · ${escapeHtml(entry.model)} · ${(entry.duration_ms / 1000).toFixed(1)} s</div>
    <div class="row"><span></span><span>
      <button data-action="copy">copy</button>
      <button data-action="delete">delete</button></span></div>
  </li>`;
}

$("#history-list").addEventListener("click", async (event) => {
  const button = (event.target as HTMLElement).closest<HTMLButtonElement>("button[data-action]");
  const row = (event.target as HTMLElement).closest<HTMLElement>("[data-history]");
  if (!button || !row) return;
  const id = Number(row.dataset.history);
  if (button.dataset.action === "copy") await api.copyHistory(id);
  if (button.dataset.action === "delete") await api.deleteHistory(id);
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

async function boot() {
  settings = await api.getSettings();
  setPhase(await api.getStatus());
  await renderFields($("#dictation-fields"), DICTATION_FIELDS);
  await renderFields($("#post-process-fields"), POST_PROCESS_FIELDS);
  await loadModels();
  await loadHistory();
}

void boot();
