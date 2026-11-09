import { readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

/**
 * Guardrail of the unification pass (design-system.md "Kiểm tra thống nhất 2026-11-06"): every
 * colour, gradient and font size comes from the tokens (packages/ui/src/tokens/tokens.css), so the
 * screens cannot drift back into two visual languages. Fails on:
 * - CSS: a raw colour (#hex, rgb()/hsl()/oklch()…, a named colour), a gradient, a font size that is
 *   not a `--fs-*` token, a `999px` pill radius or a `44px` control size (`--radius-pill`,
 *   `--size-control`);
 * - TSX/TS: a Tailwind palette colour (bg-white, text-red-600…), an arbitrary colour, gradient,
 *   radius or font size in a class (`bg-[#…]`, `rounded-[10px]`, `text-[14px]`), a hex string.
 * Only tokens.css may define raw values. A genuine exception goes in ALLOWED with its reason.
 */
const here = dirname(fileURLToPath(import.meta.url));
const webSrc = resolve(here, "..");
const uiSrc = resolve(here, "../../../../packages/ui/src");
const TOKEN_FILE = "packages/ui/src/tokens/tokens.css";

const ALLOWED: { file: string; match: string; why: string }[] = [
  { file: "features/auth/QrCode.tsx", match: "bg-white", why: "a QR code must be pure black on white to scan" },
  { file: "features/auth/QrCode.tsx", match: "text-black", why: "a QR code must be pure black on white to scan" },
  { file: "app/HomePage.tsx", match: "text-[28px]", why: "Lora greeting of the welcome band (brand spot, rulebook 1)" },
];

function files(dir: string, ext: RegExp): string[] {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) return files(path, ext);
    return ext.test(name) && !/\.test\.tsx?$/.test(name) ? [path] : [];
  });
}

const NAMED = "white|black|red|green|blue|gray|grey|silver|maroon|gold|yellow|orange|brown|purple|pink|navy|beige|ivory|wheat|tan";
const CSS_RULES: [string, RegExp][] = [
  ["raw hex colour", /#[0-9a-f]{3,8}\b/gi],
  ["raw colour function", /\b(?:rgba?|hsla?|hwb|lab|lch|oklab|oklch)\(/gi],
  ["gradient", /\b(?:repeating-)?(?:linear|radial|conic)-gradient\(/gi],
  ["named colour", new RegExp(String.raw`(?:color|background(?:-color)?|border(?:-[a-z]+)?-?color|fill|stroke|outline-color)\s*:\s*(?:${NAMED})\b`, "gi")],
  ["font size off the scale", /font-size:\s*(?!\s|var\(--fs-|inherit\b|1em\b)[^;]+;/gi],
  // The control scale and the pill shape are tokens too (audit 2026-11-09): one place to change them.
  ["raw pill radius (use var(--radius-pill))", /(?<![\w.-])999px\b/g],
  ["raw control size (use var(--size-control))", /(?<![\w.-])44px\b/g],
];
const PALETTE =
  "slate|gray|zinc|neutral|stone|red|orange|amber|yellow|lime|green|emerald|teal|cyan|sky|blue|indigo|violet|purple|fuchsia|pink|rose";
const UTIL = "bg|text|border(?:-[trblxy])?|ring|ring-offset|fill|stroke|from|via|to|outline|divide|shadow|accent|decoration|caret|placeholder";
const CODE_RULES: [string, RegExp][] = [
  ["Tailwind palette colour", new RegExp(String.raw`(?<![\w-])(?:[a-z]+:)*(?:${UTIL})-(?:white|black|(?:${PALETTE})-\d{2,3})\b`, "g")],
  ["arbitrary colour or gradient", /(?<![\w-])(?:[a-z]+:)*[a-z-]+-\[(?:#|rgb|hsl|oklch|(?:linear|radial|conic)-gradient)[^\]]*\]/g],
  ["Tailwind gradient utility", /(?<![\w-])bg-(?:gradient|linear|radial|conic)-[\w-]+/g],
  ["arbitrary radius (use rounded-sm / -md / -lg)", /(?<![\w-])rounded(?:-[a-z]+)?-\[[^\]]+\]/g],
  ["arbitrary font size (use text-xs…2xl = --fs-*)", /(?<![\w-])(?:[a-z]+:)*text-\[\d[^\]]*\]/g],
  ["hex colour string", /["'`]#[0-9a-f]{3,8}["'`]/gi],
];

function violations(path: string, text: string, rules: [string, RegExp][], base: string): string[] {
  const rel = relative(base, path).replaceAll("\\", "/");
  const out: string[] = [];
  text.split("\n").forEach((line, i) => {
    for (const [what, re] of rules) {
      for (const m of line.matchAll(re)) {
        if (ALLOWED.some((a) => a.file === rel && m[0].includes(a.match))) continue;
        out.push(`${rel}:${String(i + 1)} ${what}: ${m[0]}`);
      }
    }
  });
  return out;
}

/** Comments and url(…) (inline SVG masks) say nothing about the rendered colours. */
function stripCss(css: string): string {
  return css
    .replace(/\/\*[\s\S]*?\*\//g, (c) => c.replace(/[^\n]/g, " "))
    .replace(/url\([^)]*\)/g, "url()");
}

// Reads every source file: give it room on a loaded runner (a slow disk took 7 s once).
describe("colours, gradients and type come from the tokens only", { timeout: 30_000 }, () => {
  it("no CSS file outside tokens.css writes a raw colour, gradient or font size", () => {
    const css = [...files(webSrc, /\.css$/), ...files(uiSrc, /\.css$/)].filter(
      (p) => !p.replaceAll("\\", "/").endsWith(TOKEN_FILE),
    );
    expect(css.length).toBeGreaterThan(5);
    const found = css.flatMap((p) => violations(p, stripCss(readFileSync(p, "utf8")), CSS_RULES, webSrc));
    expect(found).toEqual([]);
  });

  it("no component writes a palette colour, arbitrary colour/radius/size or hex string", () => {
    const code = files(webSrc, /\.(tsx|ts)$/);
    const found = code.flatMap((p) => violations(p, readFileSync(p, "utf8"), CODE_RULES, webSrc));
    expect(found).toEqual([]);
  });

  it("every allowed exception still exists (no stale allowlist)", () => {
    for (const a of ALLOWED) {
      expect(readFileSync(join(webSrc, a.file), "utf8"), a.file).toContain(a.match);
    }
  });

  it("the checks catch what they guard against", () => {
    const css = ".x { color: #fff; background: linear-gradient(red, blue); font-size: 14px; border-color: white; }";
    expect(violations("a.css", css, CSS_RULES, "")).toHaveLength(4);
    const tsx = 'className="bg-white text-red-600 rounded-[10px] text-[13px] bg-[#abc] hover:bg-gradient-to-r" fill="#123456"';
    expect(violations("a.tsx", tsx, CODE_RULES, "").length).toBeGreaterThanOrEqual(7);
    expect(violations("a.css", ".x { color: var(--text); font-size: var(--fs-sm); }", CSS_RULES, "")).toEqual([]);
    expect(violations("a.css", ".x { border-radius: 999px; min-height: 44px; width: 144px; }", CSS_RULES, "")).toHaveLength(2);
  });
});
