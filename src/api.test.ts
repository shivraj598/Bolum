import { describe, expect, it } from "vitest";

import { levelPercent, settingValue } from "./api";

describe("levelPercent", () => {
  it("maps silence to zero and full scale to the whole meter", () => {
    expect(levelPercent(0)).toBe(0);
    expect(levelPercent(1)).toBe(100);
  });

  it("lifts quiet speech with a square-root curve", () => {
    expect(levelPercent(0.04)).toBe(20);
    expect(levelPercent(0.25)).toBe(50);
  });

  it("clamps levels above full scale", () => {
    expect(levelPercent(4)).toBe(100);
  });
});

describe("settingValue", () => {
  const settings = {
    model: "whisper-base.en",
    post_process: { enabled: true, base_url: "http://localhost:11434/v1" },
  };

  it("reads top-level keys", () => {
    expect(settingValue(settings, "model")).toBe("whisper-base.en");
  });

  it("reads dotted keys from nested objects", () => {
    expect(settingValue(settings, "post_process.enabled")).toBe(true);
    expect(settingValue(settings, "post_process.base_url")).toBe("http://localhost:11434/v1");
  });

  it("returns undefined for missing keys at any depth", () => {
    expect(settingValue(settings, "theme")).toBeUndefined();
    expect(settingValue(settings, "post_process.prompt")).toBeUndefined();
    expect(settingValue(settings, "model.name.deeper")).toBeUndefined();
  });
});
