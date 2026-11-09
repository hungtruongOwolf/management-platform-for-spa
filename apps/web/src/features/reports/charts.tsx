/**
 * Hand-rolled part-to-whole bar (no chart library; the trend is LineChart.tsx). Every value is also
 * in the legend as text, and the bar's accessible name lists them.
 */

export interface Part {
  label: string;
  value: number;
  tone: "primary" | "muted";
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
