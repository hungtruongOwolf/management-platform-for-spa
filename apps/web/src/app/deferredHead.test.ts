import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { declareDeferredHead } from "./deferredHead";

// Path through a variable: Vite rewrites a literal `new URL("…", import.meta.url)` into an asset URL.
const read = (path: string) => readFileSync(fileURLToPath(new URL(path, import.meta.url)), "utf8");
const html = read("../../index.html");
const page = () => new DOMParser().parseFromString(html, "text/html");
const hrefs = (doc: Document, rel: string) =>
  [...doc.head.querySelectorAll<HTMLLinkElement>(`link[rel="${rel}"]`)].map((l) => l.getAttribute("href"));

// The tab icons and the manifest wait for the first paint (frontend.md "Perf budget", /quay LCP).
describe("deferred head links", () => {
  it("ships only a placeholder icon and no manifest in the initial head", () => {
    const doc = page();
    expect(hrefs(doc, "icon")).toEqual(["data:,"]);
    expect(hrefs(doc, "manifest")).toEqual([]);
    expect(hrefs(doc, "apple-touch-icon")).toEqual(["/apple-touch-icon.png?v=5"]);
  });

  it("declares the real icons and the manifest, dropping the placeholder", () => {
    const doc = page();
    declareDeferredHead(doc);
    expect(hrefs(doc, "icon")).toEqual([
      "/favicon.ico?v=5",
      "/favicon.svg?v=5",
      "/favicon-32.png?v=5",
      "/favicon-16.png?v=5",
    ]);
    expect(hrefs(doc, "manifest")).toEqual(["/manifest.webmanifest"]);
    expect(doc.getElementById("deferred-head")).toBeNull();
  });

  it("is idempotent", () => {
    const doc = page();
    declareDeferredHead(doc);
    declareDeferredHead(doc);
    expect(hrefs(doc, "icon")).toHaveLength(4);
    expect(hrefs(doc, "manifest")).toHaveLength(1);
  });
});
