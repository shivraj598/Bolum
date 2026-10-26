import { describe, expect, it } from "vitest";

import { acceleratorKey, escapeHtml, hintHtml, isUsableShortcut, keycaps, scoreSteps, sizeLabel } from "./format";

describe("keycaps", () => {
  it("shows macOS modifier symbols", () => {
    expect(keycaps("ctrl+alt+space", true)).toEqual(["⌃", "⌥", "Space"]);
    expect(keycaps("super+shift+k", true)).toEqual(["⌘", "⇧", "K"]);
  });

  it("spells modifiers out elsewhere", () => {
    expect(keycaps("ctrl+alt+space", false)).toEqual(["Ctrl", "Alt", "Space"]);
  });

  it("accepts the aliases the shortcut plugin understands", () => {
    expect(keycaps("Command+Option+Control+F5", true)).toEqual(["⌘", "⌥", "⌃", "F5"]);
    expect(keycaps("meta+a", false)).toEqual(["Super", "A"]);
  });

  it("ignores stray separators and whitespace", () => {
    expect(keycaps(" ctrl + +space ", false)).toEqual(["Ctrl", "Space"]);
    expect(keycaps("", true)).toEqual([]);
  });
});

describe("acceleratorKey", () => {
  it("maps letters and digits to their plain names", () => {
    expect(acceleratorKey("KeyA")).toBe("a");
    expect(acceleratorKey("Digit7")).toBe("7");
  });

  it("lowercases other physical keys", () => {
    expect(acceleratorKey("Space")).toBe("space");
    expect(acceleratorKey("F12")).toBe("f12");
    expect(acceleratorKey("BracketLeft")).toBe("bracketleft");
  });

  it("returns null for modifier keys pressed on their own", () => {
    for (const code of ["ShiftLeft", "ShiftRight", "ControlLeft", "AltRight", "MetaLeft", "OSLeft"]) {
      expect(acceleratorKey(code)).toBeNull();
    }
  });
});

describe("isUsableShortcut", () => {
  it("accepts chords with Ctrl, Alt or Super", () => {
    expect(isUsableShortcut(["ctrl", "alt"], "space")).toBe(true);
    expect(isUsableShortcut(["super", "shift"], "d")).toBe(true);
  });

  it("rejects Shift alone, which would swallow capital letters", () => {
    expect(isUsableShortcut(["shift"], "u")).toBe(false);
    expect(isUsableShortcut([], "u")).toBe(false);
  });

  it("lets function keys stand alone", () => {
    expect(isUsableShortcut([], "f9")).toBe(true);
    expect(isUsableShortcut(["shift"], "f13")).toBe(true);
  });
});

describe("escapeHtml", () => {
  it("escapes every HTML-significant character", () => {
    expect(escapeHtml(`<img src="x" onerror='y'>&`)).toBe("&lt;img src=&quot;x&quot; onerror=&#39;y&#39;&gt;&amp;");
  });

  it("leaves plain text alone", () => {
    expect(escapeHtml("Whisper Base (English) — 60 MB")).toBe("Whisper Base (English) — 60 MB");
  });
});

describe("hintHtml", () => {
  it("renders backticked spans as code", () => {
    expect(hintHtml("Bind `bolum toggle` instead.")).toBe("Bind <code>bolum toggle</code> instead.");
  });

  it("escapes before adding markup, so hints cannot inject HTML", () => {
    expect(hintHtml("`<b>` & more")).toBe("<code>&lt;b&gt;</code> &amp; more");
  });
});

describe("sizeLabel", () => {
  it("uses megabytes below a gigabyte", () => {
    expect(sizeLabel(44)).toBe("44 MB");
    expect(sizeLabel(999)).toBe("999 MB");
  });

  it("switches to gigabytes with one decimal", () => {
    expect(sizeLabel(1000)).toBe("1.0 GB");
    expect(sizeLabel(1549)).toBe("1.5 GB");
  });
});

describe("scoreSteps", () => {
  it("rounds a 0..1 score to five steps", () => {
    expect(scoreSteps(0.55)).toBe(3);
    expect(scoreSteps(0.88)).toBe(4);
    expect(scoreSteps(1)).toBe(5);
  });

  it("always fills at least one step and never more than five", () => {
    expect(scoreSteps(0)).toBe(1);
    expect(scoreSteps(1.4)).toBe(5);
  });
});
