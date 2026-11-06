import { encode } from "uqr";

/**
 * QR code drawn locally as SVG from `value` (the otpauth URI never leaves the browser, and no
 * external image host is needed: CSP stays `img-src 'self' data:`).
 */
export function QrCode({ value, label }: { value: string; label: string }) {
  const { data, size } = encode(value, { ecc: "M", border: 2 });
  const cells: string[] = [];
  data.forEach((row, y) => {
    row.forEach((dark, x) => {
      if (dark) cells.push(`M${String(x)} ${String(y)}h1v1h-1z`);
    });
  });
  return (
    <svg
      role="img"
      aria-label={label}
      viewBox={`0 0 ${String(size)} ${String(size)}`}
      shapeRendering="crispEdges"
      // Always black on white, also in dark mode: scanners need that contrast.
      className="h-48 w-48 rounded-sm bg-white text-black"
    >
      <path d={cells.join("")} fill="currentColor" />
    </svg>
  );
}
