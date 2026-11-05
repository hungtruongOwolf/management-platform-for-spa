/**
 * The spa's real logo (docs for reference/logo …png), traced from its alpha channel by
 * scripts/brand/build_logo_assets.py. Each part is an SVG file used as a CSS mask (styles.css
 * `.brand-*`), so it takes the colour of its context (`currentColor`: gold on the maroon chrome,
 * maroon on cream) and costs no JavaScript. Decorative: real text next to it carries the name.
 */

/** Ring, lotus, leaves and yin-yang — the compact mark (header, sidebar, empty states). */
export function BrandMark({ className = "" }: { className?: string }) {
  return <span aria-hidden="true" className={`brand-mark ${className}`} />;
}

/** The script wordmark "Viên Phúc / Y Đường" (two lines, aspect ≈ 2:1). */
export function BrandWordmark({ className = "" }: { className?: string }) {
  return <span aria-hidden="true" className={`brand-wordmark ${className}`} />;
}

/** The whole emblem with the curved tagline: sign-in and large empty states only (~12 KB gzip). */
export function BrandEmblem({ className = "" }: { className?: string }) {
  return <span aria-hidden="true" className={`brand-emblem ${className}`} />;
}
