import { Fragment, type MouseEvent, type ReactNode } from "react";
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
  /** Buttons at the foot of the card, right-aligned (call, "Đã liên hệ", "Sửa"). */
  | "actions"
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
  /** The desktop cell is the row's header (`<th scope="row">`), e.g. the day of a daily table. */
  rowHeader?: boolean;
  /** Leave this field out of a card when it has nothing to say (a table keeps its "—"). */
  cardOmit?: (row: T) => boolean;
  /** Desktop column width ("72px", "24%"). Any width switches the table to a fixed layout: every row keeps the same columns and long text wraps inside its own. */
  width?: string;
}

export interface ResponsiveTableProps<T> {
  /** Accessible name: the table caption (visually hidden) or the card list's label. */
  caption: string;
  rows: readonly T[];
  rowKey: (row: T) => string;
  columns: readonly Column<T>[];
  /** Class per row/card, e.g. a voided line. */
  rowClassName?: (row: T) => string | undefined;
  /** Cards below this width (default 640 = phones). Wide tables (6+ columns) use 1024: on a tablet with the icon rail they would squeeze every cell to two lines. */
  cardsBelow?: 640 | 768 | 1024;
  /** The first link in the card title covers the whole card (one tap target, Polaris ResourceList). */
  stretchLink?: boolean;
  /** Desktop `<tfoot>` row(s); on phones shown under the cards as given. */
  footer?: { table: ReactNode; cards: ReactNode };
  className?: string;
  /**
   * A click on the row (or card) outside its controls, e.g. to open the row's details. Mouse and
   * touch only: the row must also hold a real control doing the same for keyboards.
   */
  onRowClick?: (row: T) => void;
  /** Content under a row (its details, an inline form): a full-width row, or the card's foot. */
  expanded?: (row: T) => ReactNode;
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
  onRowClick,
  expanded,
}: ResponsiveTableProps<T>) {
  const cards = useMediaQuery(`(max-width: ${String(cardsBelow - 0.02)}px)`);
  const clickRow = onRowClick
    ? (row: T) => (e: MouseEvent) => {
        // A control in the row does its own thing; a drag that selected text is not a click.
        const onControl = (e.target as Element).closest("a, button, input, select, textarea, label, [role=menu]");
        if (!onControl && !window.getSelection()?.toString()) onRowClick(row);
      }
    : undefined;
  const fixed = columns.some((c) => c.width);
  if (!cards) {
    return (
      // The wrapper scrolls (sticky header, long lists): focusable and named so keyboards can too.
      <div className={`table-wrap ${className}`} tabIndex={0} role="region" aria-label={caption}>
        <table className={`data-table ${fixed ? "data-table-fixed" : ""}`}>
          <caption className="sr-only">{caption}</caption>
          {fixed && (
            <colgroup>
              {columns.map((c) => (
                <col key={c.id} style={c.width ? { width: c.width } : undefined} />
              ))}
            </colgroup>
          )}
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
            {rows.map((row) => {
              const more = expanded?.(row);
              const rowCls = [rowClassName?.(row), onRowClick ? "rt-clickable" : "", more ? "rt-open" : ""];
              return (
                <Fragment key={rowKey(row)}>
                  <tr className={rowCls.join(" ").trim() || undefined} onClick={clickRow?.(row)}>
                    {columns.map((c) => {
                      const cls = [c.numeric ? "num" : "", c.className ?? ""].join(" ").trim() || undefined;
                      return c.rowHeader ? (
                        <th key={c.id} scope="row" className={cls}>
                          {c.cell(row)}
                        </th>
                      ) : (
                        <td key={c.id} className={cls}>
                          {c.cell(row)}
                        </td>
                      );
                    })}
                  </tr>
                  {more && (
                    <tr className="rt-expanded">
                      <td colSpan={columns.length}>{more}</td>
                    </tr>
                  )}
                </Fragment>
              );
            })}
          </tbody>
          {footer && <tfoot>{footer.table}</tfoot>}
        </table>
      </div>
    );
  }

  const slot = (s: CardSlot) => columns.filter((c) => (c.card ?? "field") === s);
  const [title, subtitle, aside, fields, actions] = [slot("title"), slot("subtitle"), slot("aside"), slot("field"), slot("actions")];
  return (
    <div className={`rt-cards-wrap ${className}`}>
      <ul aria-label={caption} className={`rt-cards ${stretchLink ? "rt-stretch" : ""}`}>
        {rows.map((row) => (
          <li
            key={rowKey(row)}
            className={`rt-card ${rowClassName?.(row) ?? ""} ${onRowClick ? "rt-clickable" : ""}`}
            onClick={clickRow?.(row)}
          >
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
                {fields.filter((c) => !c.cardOmit?.(row)).map((c) => (
                  <div key={c.id}>
                    <dt>{c.cardLabel ?? c.header}</dt>
                    <dd className={c.numeric ? "tabular" : undefined}>{c.cell(row)}</dd>
                  </div>
                ))}
              </dl>
            )}
            {actions.length > 0 && (
              <div className="rt-card-actions">
                {actions.map((c) => (
                  <span key={c.id}>{c.cell(row)}</span>
                ))}
              </div>
            )}
            {expanded?.(row)}
          </li>
        ))}
      </ul>
      {footer?.cards}
    </div>
  );
}
