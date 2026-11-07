import { percentOfBp } from "./labels";
import type { WorkTable as Table } from "./payslipWork";
import { vnd } from "./ui";

/**
 * A payslip's work table (EM 2026-11-07): blocks by type, each item once with its count and
 * value, a subtotal per block, then the total the payslip computes from and the commission it
 * becomes. Three fixed columns (name, count, value) so the numbers sit together and stay tabular;
 * a phone keeps the same table, names wrap.
 */
export function WorkTable({
  title,
  table,
  itemHeader,
  qtyHeader,
  commissionLabel,
}: {
  title: string;
  table: Table;
  itemHeader: string;
  qtyHeader: string;
  commissionLabel: string;
}) {
  if (table.groups.length === 0) return null;
  return (
    <section className="grid gap-2" aria-label={title}>
      <h3 className="text-sm font-semibold">{title}</h3>
      <div className="payslip overflow-hidden">
        <table className="work-table">
          <caption className="sr-only">{title}</caption>
          <colgroup>
            <col />
            <col className="work-col-qty" />
            <col className="work-col-value" />
          </colgroup>
          <thead>
            <tr>
              <th scope="col">{itemHeader}</th>
              <th scope="col" className="num">
                {qtyHeader}
              </th>
              <th scope="col" className="num">
                Giá trị
              </th>
            </tr>
          </thead>
          {table.groups.map((g) => (
            <tbody key={g.key}>
              <tr className="work-group">
                <th scope="rowgroup" colSpan={3}>
                  {g.label}
                </th>
              </tr>
              {g.rows.map((r) => (
                <tr key={r.name}>
                  <th scope="row">{r.name}</th>
                  <td className="num">{r.qty}</td>
                  <td className="num">{vnd(r.value)}</td>
                </tr>
              ))}
              {table.groups.length > 1 && (
                <tr className="work-subtotal">
                  <th scope="row">Cộng {g.label.toLowerCase()}</th>
                  <td className="num">{g.qty}</td>
                  <td className="num">{vnd(g.value)}</td>
                </tr>
              )}
            </tbody>
          ))}
          <tfoot>
            <tr className="work-total">
              <th scope="row">Tổng tính hoa hồng</th>
              <td className="num">{table.qty}</td>
              <td className="num">{vnd(table.value)}</td>
            </tr>
            <tr className="work-commission">
              <th scope="row" colSpan={2}>
                {commissionLabel}{" "}
                <span className="block text-xs text-text-muted">
                  {percentOfBp(table.rateBp)} × {vnd(table.value)}
                </span>
              </th>
              <td className="num">{vnd(table.commission)}</td>
            </tr>
          </tfoot>
        </table>
      </div>
    </section>
  );
}
