/**
 * Hand-rolled charts: HTML bars sized in %, no chart library (the reports chunk stays a few KB).
 * One hue (maroon) for the series and gold for the highlighted bar; values are always available as
 * text, so the chart is a list for screen readers rather than a picture.
 */

export interface Bar {
  key: string;
  label: string;
  sub?: string;
  value: number;
}

export function BarChart({
  bars,
  format,
  highlight,
  label,
}: {
  bars: Bar[];
  format: (n: number) => string;
  highlight?: string;
  label: string;
}) {
  const max = Math.max(1, ...bars.map((b) => b.value));
  return (
    <figure className="m-0">
      <ol aria-label={label} className="chart-bars">
        {bars.map((b, i) => (
          <li key={b.key} className="chart-col" tabIndex={0} aria-label={`${b.sub ?? ""} ${b.label}: ${format(b.value)}`}>
            <span className="chart-value tabular" aria-hidden="true">
              {format(b.value)}
            </span>
            <span aria-hidden="true" className="chart-plot">
              <span
                className={`chart-bar ${b.key === highlight ? "is-highlight" : ""}`}
                style={{ height: `${String(Math.max(2, (b.value / max) * 100))}%`, animationDelay: `${String(i * 40)}ms` }}
              />
            </span>
            <span className="chart-label" aria-hidden="true">
              <span className="block font-semibold">{b.sub}</span>
              {b.label}
            </span>
          </li>
        ))}
      </ol>
    </figure>
  );
}

export interface Part {
  label: string;
  value: number;
  tone: "primary" | "accent" | "muted";
}

/** One horizontal bar split into parts (cash / transfer, package / one-off) with a legend. */
export function SplitBar({ parts, format, label }: { parts: Part[]; format: (n: number) => string; label: string }) {
  const total = parts.reduce((s, p) => s + Math.max(0, p.value), 0);
  return (
    <div>
      <div className="split-bar" role="img" aria-label={`${label}: ${parts.map((p) => `${p.label} ${format(p.value)}`).join(", ")}`}>
        {parts.map((p) => (
          <span
            key={p.label}
            className={`split-part tone-${p.tone}`}
            style={{ width: `${String(total ? (Math.max(0, p.value) / total) * 100 : 0)}%` }}
          />
        ))}
      </div>
      <ul className="mt-3 grid gap-2 text-sm">
        {parts.map((p) => (
          <li key={p.label} className="flex items-center gap-2">
            <span aria-hidden="true" className={`size-2.5 rounded-full tone-${p.tone}`} />
            <span className="text-text-muted">{p.label}</span>
            <span className="ml-auto font-semibold tabular">{format(p.value)}</span>
            <span className="w-11 text-right text-xs text-text-muted tabular">
              {total ? `${String(Math.round((Math.max(0, p.value) / total) * 100))}%` : "—"}
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}
