import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

/**
 * WCAG 2.2 AA for the palette (design-system.md "Màu"): 4.5:1 for text, 3:1 for UI parts (focus
 * ring, input borders, decorative gold that carries meaning). Reads tokens.css itself, so a token
 * edit that breaks contrast fails here, in light and dark.
 */
const css = readFileSync(new URL("./tokens.css", import.meta.url), "utf8");

function block(selector: string): Record<string, string> {
  const start = css.indexOf(`${selector} {`);
  if (start < 0) throw new Error(`no block ${selector}`);
  const body = css.slice(start + selector.length + 2, css.indexOf("}", start));
  const tokens: Record<string, string> = {};
  for (const [, name, value] of body.matchAll(/--([a-z0-9-]+):\s*([^;]+);/g)) {
    if (name && value) tokens[name] = value.trim();
  }
  return tokens;
}

const light = block(":root");
const dark = block(':root:not([data-theme="light"])');
const darkManual = block(':root[data-theme="dark"]');

function luminance(hex: string): number {
  const match = /^#([0-9a-f]{6})$/i.exec(hex);
  if (!match?.[1]) throw new Error(`not a 6-digit hex colour: ${hex}`);
  const channels = (match[1].match(/../g) ?? []).map((c) => {
    const v = parseInt(c, 16) / 255;
    return v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
  });
  const [r = 0, g = 0, b = 0] = channels;
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function contrast(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x) as [number, number];
  return (hi + 0.05) / (lo + 0.05);
}

const TEXT = 4.5;
const UI = 3;

/** [foreground, background, minimum] — every pairing the components actually use. */
const PAIRS: [string, string, number][] = [
  ["text", "bg", TEXT],
  ["text", "surface", TEXT],
  ["text", "surface-2", TEXT],
  ["text-muted", "bg", TEXT],
  ["text-muted", "surface", TEXT],
  ["text-muted", "surface-2", TEXT],
  ["primary", "surface", TEXT],
  ["primary", "bg", TEXT],
  ["primary", "primary-soft", TEXT],
  ["text", "primary-soft", TEXT],
  ["on-primary", "primary", TEXT],
  ["on-primary", "primary-hover", TEXT],
  ["accent-text", "surface", TEXT],
  ["accent-text", "bg", TEXT],
  ["accent-text", "accent-soft", TEXT],
  ["accent", "surface", UI],
  ["accent", "bg", UI],
  ["focus", "surface", UI],
  ["focus", "bg", UI],
  ["border-strong", "surface", UI],
  ["danger", "surface", TEXT],
  ["danger", "bg", TEXT],
  ["danger", "danger-soft", TEXT],
  ["warning-text", "warning-bg", TEXT],
  ["success", "surface", TEXT],
  ["success", "success-soft", TEXT],
  ["info", "surface", TEXT],
  ["info", "info-soft", TEXT],
  ["chrome-text", "chrome-bg", TEXT],
  ["chrome-text", "chrome-bg-2", TEXT],
  ["chrome-text-muted", "chrome-bg", TEXT],
  ["chrome-text-muted", "chrome-bg-2", TEXT],
  ["chrome-accent", "chrome-bg", TEXT],
  ["chrome-accent", "chrome-bg-2", TEXT],
  // The whole logo (lettering + tagline) on every light/dark surface it sits on.
  ["brand-ink", "bg", TEXT],
  ["brand-ink", "surface", TEXT],
  ["brand-ink", "surface-2", TEXT],
  // Focus ring next to the controls it outlines (WCAG 2.4.13 / 1.4.11).
  ["focus", "surface-2", UI],
];

describe.each([
  ["light", light],
  ["dark", dark],
])("%s palette", (_, tokens) => {
  it.each(PAIRS)("%s on %s ≥ %s:1", (fg, bg, min) => {
    const a = tokens[fg];
    const b = tokens[bg];
    expect(a, `--${fg}`).toBeDefined();
    expect(b, `--${bg}`).toBeDefined();
    expect(contrast(a ?? "", b ?? "")).toBeGreaterThanOrEqual(min);
  });
});

describe("dark theme blocks", () => {
  it("the manual data-theme=dark block repeats the media-query block exactly", () => {
    expect(darkManual).toEqual(dark);
  });

  it("dark defines every light token except the shared scales", () => {
    const shared = /^(radius|dur|ease|font)-/;
    const missing = Object.keys(light).filter((k) => !shared.test(k) && !(k in dark));
    expect(missing).toEqual([]);
  });
});
