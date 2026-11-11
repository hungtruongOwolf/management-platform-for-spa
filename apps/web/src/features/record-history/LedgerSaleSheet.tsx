// "Sổ doanh thu": a row's own history in a sheet (design record-history §2.4). The sheet exists
// only to show it (every other fact is on the row already), so the history is open from the start.
import type { LedgerRow } from "@vpyd/api-client";
import { formatVnd } from "@vpyd/ui";
import { BottomSheet } from "../../components/BottomSheet";
import { spaTime } from "../../lib/format";
import { subjectName } from "../activity/sentence";
import { RecordTrace } from "./RecordTrace";
import { fullDay } from "./text";

/** What a row of the book is, as the sheet's title says it. */
export function ledgerRowTitle(r: Pick<LedgerRow, "entry" | "kind">): string {
  if (r.entry === "refund") return "Hoàn tiền gói";
  if (r.entry === "void") return "Hủy hóa đơn";
  return r.kind === "package" ? "Hóa đơn bán gói" : "Hóa đơn dịch vụ";
}

/** "chị Lê Thị Mai", "khách vãng lai" (the sentence words of "Nhật ký thao tác"). */
export function ledgerCustomer(r: Pick<LedgerRow, "customer_id" | "customer_name" | "customer_gender">): string {
  return subjectName({ kind: "customer", id: r.customer_id, name: r.customer_name, gender: r.customer_gender ?? null });
}

function paymentText(r: LedgerRow): string {
  const parts = [r.cash ? `Tiền mặt ${formatVnd(r.cash)}` : "", r.bank_transfer ? `Chuyển khoản ${formatVnd(r.bank_transfer)}` : ""];
  return parts.filter(Boolean).join(" · ");
}

export interface LedgerSaleSheetProps {
  row: LedgerRow;
  open: boolean;
  onClose: () => void;
  /** The history right was taken away (`403`): the sheet closes, focus goes back to the row. */
  onForbidden: () => void;
}

export function LedgerSaleSheet({ row, open, onClose, onForbidden }: LedgerSaleSheetProps) {
  const what = [row.one_off, row.package].filter(Boolean).join(" · ");
  const amount = row.cash + row.bank_transfer + row.unpaid;
  const payment = paymentText(row);
  return (
    <BottomSheet
      open={open}
      onClose={onClose}
      title={`${ledgerRowTitle(row)} · ${ledgerCustomer(row)}`}
      description={fullDay(row.business_date)}
      footer={
        <button type="button" className="btn btn-secondary" onClick={onClose}>
          Đóng
        </button>
      }
    >
      <div className="grid gap-3">
        <dl className="grid gap-1 text-sm">
          <div className="flex items-baseline justify-between gap-3">
            <dt className="min-w-0 [overflow-wrap:anywhere]">{what || ledgerRowTitle(row)}</dt>
            <dd className="shrink-0 tabular font-semibold">{formatVnd(amount)}</dd>
          </div>
          {row.beneficiary_name && (
            <div>
              <dt className="sr-only">Mua cho</dt>
              <dd className="text-text-muted">Mua cho {row.beneficiary_name}</dd>
            </div>
          )}
          {payment && (
            <div>
              <dt className="sr-only">Thanh toán</dt>
              <dd className="text-text-muted tabular">{payment}</dd>
            </div>
          )}
          {row.note && (
            <div>
              <dt className="sr-only">Ghi chú</dt>
              <dd className="text-text-muted [overflow-wrap:anywhere]">{row.note}</dd>
            </div>
          )}
        </dl>
        {open && (
          <RecordTrace
            record={{ type: row.entry === "refund" ? "refund" : "sale", id: row.id }}
            noun={`${ledgerRowTitle(row).toLowerCase()} ${spaTime(row.at)} của ${ledgerCustomer(row)}`}
            day={row.business_date}
            defaultOpen
            onForbidden={onForbidden}
          />
        )}
      </div>
    </BottomSheet>
  );
}
