// Web-font loading policy (frontend.md, "Perf budget"). The six font files weigh about as much as
// the main bundle; fetched up front, they finished before the first paint on every cold load and
// made it wait (≈ 0.7 s of LCP on Lighthouse's slow 4G). So:
// - cold load: text first paints in the system fallback from the tokens, and `fonts.css` is
//   requested once the first screen is up (font-display: swap, as before);
// - after that, a flag says the files are in the HTTP cache (hashed, immutable), and later loads
//   request them at startup so the brand font is there for the first paint, without a swap.

const CACHED_FLAG = "vpyd.fonts-cached";
/** Gap between the first rendered route and the font request, so the paint goes out first. */
const AFTER_FIRST_SCREEN_MS = 500;

let requested = false;

function load(): void {
  if (requested) return;
  requested = true;
  void import("../fonts.css")
    // `fonts` is missing in jsdom; in browsers `ready` settles once the files have downloaded.
    .then(() => (document as Partial<Document>).fonts?.ready)
    .then(() => {
      try {
        localStorage.setItem(CACHED_FLAG, "1");
      } catch {
        // Storage unavailable (private mode): every load counts as cold, which is only slower.
      }
    });
}

function fontsCached(): boolean {
  try {
    return localStorage.getItem(CACHED_FLAG) === "1";
  } catch {
    return false;
  }
}

/** Call at startup: loads the fonts now when they are cached, else returns false. */
export function loadFontsIfCached(): boolean {
  if (!fontsCached()) return false;
  load();
  return true;
}

/** Call once the first route has rendered: loads the fonts shortly after its paint. */
export function loadFontsAfterFirstScreen(): void {
  if (requested) return;
  setTimeout(load, AFTER_FIRST_SCREEN_MS);
}
