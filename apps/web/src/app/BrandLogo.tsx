/**
 * The spa's real logo (docs for reference/logo …png), traced from its alpha channel by
 * scripts/brand/build_logo_assets.py, and always used WHOLE: lotus, ring, script wordmark, yin-yang
 * with leaves and the curved tagline (EM 2026-11-05: never strip or recombine parts). One SVG file
 * used as a CSS mask (components.css `.brand-logo`), so it takes the colour of its context
 * (`--brand-ink`: maroon on cream, gold on dark and on the maroon chrome) and costs no JavaScript.
 * Decorative: real text next to it, or the link's label, carries the name.
 */
export function BrandLogo({ className = "" }: { className?: string }) {
  return <span aria-hidden="true" className={`brand-logo ${className}`} />;
}
