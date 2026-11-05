import { useId, useState, type KeyboardEvent, type PointerEvent } from "react";

/**
 * Hand-rolled SVG line chart (dataviz skill; no chart library, so the reports chunk stays small).
 * Form: emphasis. The selected period is the one brand-hue line with a 10% wash; the previous
 * period of the same length is a de-emphasis gray line on the same (single) axis, so the two
 * compare honestly. Text never wears the series colour. Crosshair + one tooltip for both series on
 * hover and on keyboard focus (left/right arrows); every value is also in the table the caller renders.
 *
 * Geometry: the SVG stretches to its box (`preserveAspectRatio="none"`) with non-scaling strokes;
 * dots, tick labels and the tooltip are HTML placed in %, so nothing is distorted at any width.
 */

export interface LinePoint {
  key: string;
  /** Axis label ("05/11", "T11/26"). */
  label: string;
  /** Tooltip heading ("T5 05/11/2026"). */
  title: string;
  value: number;
  /** Same position in the previous period, when compared. */
  previous?: number | undefined;
}

const W = 1000;
const H = 300;

/** 0, a clean max and two steps between (0 / 2 tr / 4 tr / 6 tr). */
export function niceTicks(max: number): number[] {
  if (max <= 0) return [0, 1];
  const raw = max / 3;
  const pow = 10 ** Math.floor(Math.log10(raw));
  const step = ([1, 2, 2.5, 5, 10].find((m) => m * pow >= raw) ?? 10) * pow;
  return [0, step, step * 2, step * 3];
}

function pathOf(values: number[], x: (i: number) => number, y: (v: number) => number): string {
  return values.map((v, i) => `${i === 0 ? "M" : "L"}${x(i).toFixed(1)},${y(v).toFixed(1)}`).join("");
}

export function LineChart({
  points,
  format,
  axisFormat = format,
  label,
  seriesLabel,
  previousLabel,
}: {
  points: LinePoint[];
  format: (n: number) => string;
  axisFormat?: (n: number) => string;
  label: string;
  seriesLabel: string;
  previousLabel?: string | undefined;
}) {
  const id = useId();
  const [active, setActive] = useState<number | null>(null);
  const hasPrevious = previousLabel !== undefined && points.some((p) => p.previous !== undefined);
  const ticks = niceTicks(Math.max(0, ...points.map((p) => Math.max(p.value, p.previous ?? 0))));
  const top = ticks.at(-1) ?? 1;
  const n = points.length;
  const x = (i: number) => (n <= 1 ? W / 2 : (i / (n - 1)) * W);
  const y = (v: number) => H - (Math.max(0, v) / top) * H;
  const pct = (i: number) => (x(i) / W) * 100;
  const height = (v: number) => `${String((Math.max(0, v) / top) * 100)}%`;
  const line = pathOf(
    points.map((p) => p.value),
    x,
    y,
  );
  const area = n > 0 ? `${line}L${x(n - 1).toFixed(1)},${String(H)}L${x(0).toFixed(1)},${String(H)}Z` : "";
  const prevLine = hasPrevious
    ? pathOf(
        points.map((p) => p.previous ?? 0),
        x,
        y,
      )
    : "";
  // Thin the axis labels so they never collide: at most ~7 across, ~4 on a phone.
  const every = Math.max(1, Math.ceil(n / 7));
  const everyNarrow = Math.max(1, Math.ceil(n / 4));
  const last = points.at(-1);
  const point = active !== null ? points[active] : undefined;

  const pick = (e: PointerEvent<HTMLDivElement>) => {
    const box = e.currentTarget.getBoundingClientRect();
    if (box.width === 0 || n === 0) return;
    const ratio = (e.clientX - box.left) / box.width;
    setActive(Math.min(n - 1, Math.max(0, Math.round(ratio * (n - 1)))));
  };
  const onKey = (e: KeyboardEvent<HTMLDivElement>) => {
    if (n === 0) return;
    const at = active ?? n - 1;
    const keys: Record<string, number> = { ArrowLeft: at - 1, ArrowRight: at + 1, Home: 0, End: n - 1 };
    const next = keys[e.key];
    if (next === undefined) return;
    e.preventDefault();
    setActive(Math.min(n - 1, Math.max(0, next)));
  };

  return (
    <figure className="line-chart m-0">
      {hasPrevious && (
        <ul className="line-legend" aria-label="Chú thích">
          <li>
            <span aria-hidden="true" className="line-key line-key-current" />
            {seriesLabel}
          </li>
          <li>
            <span aria-hidden="true" className="line-key line-key-previous" />
            {previousLabel}
          </li>
        </ul>
      )}
      <div className="line-frame">
        <ol className="line-yaxis" aria-hidden="true">
          {ticks.map((t) => (
            <li key={t} style={{ bottom: height(t) }}>
              {axisFormat(t)}
            </li>
          ))}
        </ol>
        <div
          className="line-plot"
          tabIndex={0}
          role="img"
          aria-label={`${label}. Phím mũi tên trái, phải để xem từng điểm; số liệu đầy đủ ở bảng bên dưới.`}
          aria-describedby={point ? `${id}-tip` : undefined}
          onPointerMove={pick}
          onPointerDown={pick}
          onPointerLeave={() => {
            setActive(null);
          }}
          onFocus={() => {
            setActive((a) => a ?? n - 1);
          }}
          onBlur={() => {
            setActive(null);
          }}
          onKeyDown={onKey}
        >
          <svg viewBox={`0 0 ${String(W)} ${String(H)}`} preserveAspectRatio="none" aria-hidden="true" focusable="false">
            {ticks.map((t) => (
              <line key={t} x1="0" x2={W} y1={y(t)} y2={y(t)} className="line-grid" />
            ))}
            {hasPrevious && <path d={prevLine} className="line-previous" />}
            <path d={area} className="line-area" />
            <path d={line} className="line-current" />
          </svg>
          {last && !point && (
            <>
              <span aria-hidden="true" className="line-dot" style={{ left: `${String(pct(n - 1))}%`, bottom: height(last.value) }} />
              <span aria-hidden="true" className="line-end-label tabular" style={{ bottom: height(last.value) }}>
                {format(last.value)}
              </span>
            </>
          )}
          {point && active !== null && (
            <>
              <span aria-hidden="true" className="line-crosshair" style={{ left: `${String(pct(active))}%` }} />
              {hasPrevious && point.previous !== undefined && (
                <span
                  aria-hidden="true"
                  className="line-dot line-dot-previous"
                  style={{ left: `${String(pct(active))}%`, bottom: height(point.previous) }}
                />
              )}
              <span aria-hidden="true" className="line-dot" style={{ left: `${String(pct(active))}%`, bottom: height(point.value) }} />
              <div
                id={`${id}-tip`}
                role="tooltip"
                className={`line-tooltip ${pct(active) > 60 ? "is-left" : ""}`}
                style={{ left: `${String(pct(active))}%` }}
              >
                <p className="text-xs text-text-muted">{point.title}</p>
                <p className="line-tip-row">
                  <span aria-hidden="true" className="line-key line-key-current" />
                  <strong className="tabular">{format(point.value)}</strong>
                  <span className="text-text-muted">{seriesLabel}</span>
                </p>
                {hasPrevious && point.previous !== undefined && (
                  <p className="line-tip-row">
                    <span aria-hidden="true" className="line-key line-key-previous" />
                    <strong className="tabular">{format(point.previous)}</strong>
                    <span className="text-text-muted">{previousLabel}</span>
                  </p>
                )}
              </div>
            </>
          )}
        </div>
        <ol className="line-xaxis" aria-hidden="true">
          {points.map((p, i) =>
            i % every === 0 || i === n - 1 ? (
              <li
                key={p.key}
                style={{ left: `${String(pct(i))}%` }}
                className={`${i === 0 ? "is-first" : i === n - 1 ? "is-last" : ""} ${i % everyNarrow === 0 || i === n - 1 ? "" : "is-minor"}`}
              >
                {p.label}
              </li>
            ) : null,
          )}
        </ol>
      </div>
    </figure>
  );
}
