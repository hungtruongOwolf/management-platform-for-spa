/**
 * The spa's mark, after the printed logo: a lotus inside a ring. Inline SVG (no request, CSP
 * 'self'), drawn in currentColor so it takes the gold of its context. Decorative: the wordmark next
 * to it is real text and carries the name.
 */
export function BrandMark({ className = "" }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 48 48"
      aria-hidden="true"
      focusable="false"
      className={className}
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <circle cx="24" cy="24" r="22" />
      <circle cx="24" cy="24" r="19.5" strokeWidth="0.8" opacity="0.7" />
      <path d="M24 12c4 4.6 4 12.4 0 18-4-5.6-4-13.4 0-18Z" />
      <path d="M24 30c-5.6-.6-9.4-4.8-10.4-10.6 5 .8 8.8 4.6 10.4 10.6Z" />
      <path d="M24 30c5.6-.6 9.4-4.8 10.4-10.6-5 .8-8.8 4.6-10.4 10.6Z" />
      <path d="M23 31.2c-5.4 1.4-10.6-.4-13.8-4.2 5-1.2 10 .4 13.8 4.2Z" />
      <path d="M25 31.2c5.4 1.4 10.6-.4 13.8-4.2-5-1.2-10 .4-13.8 4.2Z" />
      <path d="M16 35.5c5.2 1.6 10.8 1.6 16 0" />
    </svg>
  );
}
