import type { ReactNode } from "react";
import { useMediaQuery } from "./useMediaQuery";

/** Where a column goes in the phone card (design-system § Mobile patterns). */
export type CardSlot =
  /** First line, bold, left. Put the row's link here: with `stretchLink` the whole card opens it. */
  | "title"
  /** Under the title, muted, joined with " · ". */
  | "subtitle"
  /** Top right: the key number or status badge (one or two). */
  | "aside"
  /** A "label: value" row in the card body (the default). */
  | "field"
  /** Desktop only (details the card already says another way). */
  | "hidden";

export interface Column<T> {
  /** Stable id (React key). */
  id: string;
  header: ReactNode;
  cell: (row: T) => ReactNode;
  /** Numbers and money: right-aligned, tabular figures, never wrapped. */
  numeric?: boolean;
  card?: CardSlot;
  /** Label of a card field when `header` is not plain text (defaults to `header`). */
  cardLabel?: ReactNode;
  /** Extra class on the desktop cell. */
  className?: string;
}

export interface ResponsiveTableProps<T> {
  /** Accessible name: the table caption (visually hidden) or the card list's label. */
  caption: string;
  rows: readonly T[];
  rowKey: (row: T) => string;
  columns: readonly Column<T>[];
  /** Class per row/card, e.g. a voided line. */
  rowClassName?: (row: T) => string | undefined;
  /** Cards below this width (default 640 = phones). Wide tables (6+ columns) use 768. */
  cardsBelow?: 640 | 768;
  /** The first link in the card title covers the whole card (one tap target, Polaris ResourceList). */
  stretchLink?: boolean;
  /** Desktop `<tfoot>` row(s); on phones shown under the cards as given. */
  footer?: { table: ReactNode; cards: ReactNode };
  className?: string;
}

/**
 * A table on wide screens, a list of cards on phones (EM round 4: "UI của mobile đâu thể y như
 * web"). Each column says where it sits in the card, so one definition serves both and the
 * phone never scrolls sideways (Baymard: horizontal scrolling tables hide columns). The layout
 * switches in JS (`matchMedia`), so only one of the two is in the DOM and the accessibility tree.
 */
export function ResponsiveTable<T>({
  caption,
  rows,
  rowKey,
  columns,
  rowClassName,
  cardsBelow = 640,
  stretchLink = false,
  footer,
  className = "",
}: ResponsiveTableProps<T>) {
  const cards = useMediaQuery(`(max-width: ${String(cardsBelow - 0.02)}px)`);
  if (!cards) {
    return (
      <div className={`table-wrap ${className}`}>
        <table className="data-table">
          <caption className="sr-only">{caption}</caption>
          <thead>
            <tr>
              {columns.map((c) => (
                <th key={c.id} scope="col" className={c.numeric ? "num" : undefined}>
                  {c.header}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => (
              <tr key={rowKey(row)} className={rowClassName?.(row)}>
                {columns.map((c) => (
                  <td key={c.id} className={[c.numeric ? "num" : "", c.className ?? ""].join(" ").trim() || undefined}>
                    {c.cell(row)}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
          {footer && <tfoot>{footer.table}</tfoot>}
        </table>
      </div>
    );
  }

  const slot = (s: CardSlot) => columns.filter((c) => (c.card ?? "field") === s);
  const [title, subtitle, aside, fields] = [slot("title"), slot("subtitle"), slot("aside"), slot("field")];
  return (
    <div className={`rt-cards-wrap ${className}`}>
      <ul aria-label={caption} className={`rt-cards ${stretchLink ? "rt-stretch" : ""}`}>
        {rows.map((row) => (
          <li key={rowKey(row)} className={`rt-card ${rowClassName?.(row) ?? ""}`}>
            <div className="rt-card-head">
              <div className="rt-card-title">
                {title.map((c) => (
                  <span key={c.id}>{c.cell(row)}</span>
                ))}
                {subtitle.length > 0 && (
                  <span className="rt-card-sub">
                    {subtitle.map((c) => (
                      <span key={c.id}>{c.cell(row)}</span>
                    ))}
                  </span>
                )}
              </div>
              {aside.length > 0 && (
                <div className="rt-card-aside">
                  {aside.map((c) => (
                    <span key={c.id} className={c.numeric ? "tabular" : undefined}>
                      {c.cell(row)}
                    </span>
                  ))}
                </div>
              )}
            </div>
            {fields.length > 0 && (
              <dl className="rt-card-fields">
                {fields.map((c) => (
                  <div key={c.id}>
                    <dt>{c.cardLabel ?? c.header}</dt>
                    <dd className={c.numeric ? "tabular" : undefined}>{c.cell(row)}</dd>
                  </div>
                ))}
              </dl>
            )}
          </li>
        ))}
      </ul>
      {footer?.cards}
    </div>
  );
}
