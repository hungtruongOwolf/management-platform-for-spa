// Tab icons and the web manifest are declared after the first paint (frontend.md "Perf budget").
// The browser fetches favicons right after the window `load` event and the manifest's icon right
// after the manifest: on a cold load that is while the first screen is still being rendered, and
// Lighthouse's simulated LCP counts every request that ends before the paint (≈ 0.3 s of /quay LCP
// on slow 4G for ~23 KB of icons). index.html keeps the links in <template id="deferred-head">
// (so they stay declarative and reviewable) plus a `data:,` placeholder that stops the automatic
// /favicon.ico request meanwhile.

export const DEFERRED_HEAD_ID = "deferred-head";

/** Moves the deferred links into <head> and drops the placeholder icon. Idempotent. */
export function declareDeferredHead(doc: Document = document): void {
  const template = doc.getElementById(DEFERRED_HEAD_ID);
  if (!(template instanceof HTMLTemplateElement)) return;
  for (const placeholder of doc.head.querySelectorAll("link[data-placeholder]")) {
    placeholder.remove();
  }
  template.replaceWith(template.content);
}
