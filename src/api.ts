import { invoke } from "@tauri-apps/api/core";

export type ModelInfo = {
  id: string;
  name: string;
  engine: string;
  description: string;
  languages: string;
  size_mb: number;
  downloaded: boolean;
  downloading: boolean;
  progress: number;
};

export type HistoryEntry = {
  id: number;
  timestamp_ms: number;
  text: string;
  post_processed: string | null;
  model: string;
  duration_ms: number;
};

export type Phase = "idle" | "listening" | "working";

export type Settings = Record<string, unknown> & {
  post_process?: Record<string, unknown>;
};

export const api = {
  getSettings: () => invoke<Settings>("get_settings"),
  setSetting: (key: string, value: unknown) => invoke<Settings>("set_setting", { key, value }),
  getStatus: () => invoke<Phase>("get_status"),
  getMicrophones: () => invoke<string[]>("get_microphones"),
  getTypingTools: () => invoke<string[]>("get_typing_tools"),

  listModels: () => invoke<ModelInfo[]>("list_models"),
  downloadModel: (id: string) => invoke<void>("download_model", { id }),
  cancelDownload: (id: string) => invoke<void>("cancel_download", { id }),
  deleteModel: (id: string) => invoke<void>("delete_model", { id }),

  startRecording: () => invoke<string>("start_recording"),
  stopRecording: () => invoke<string>("stop_recording"),
  cancelRecording: () => invoke<string>("cancel_recording"),

  listHistory: () => invoke<HistoryEntry[]>("list_history"),
  deleteHistory: (id: number) => invoke<void>("delete_history", { id }),
  copyHistory: (id: number) => invoke<void>("copy_history", { id }),
};

/** Maps a 0..1 RMS level to a 0..100 meter width, shared by both windows. */
export function levelPercent(level: number): number {
  return Math.min(100, Math.round(Math.sqrt(level) * 100));
}

/** Reads a possibly dotted key such as `post_process.enabled`. */
export function settingValue(settings: Settings, key: string): unknown {
  return key
    .split(".")
    .reduce<unknown>((value, part) => (value as Record<string, unknown>)?.[part], settings);
}
