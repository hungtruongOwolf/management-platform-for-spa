// Package sales of the day saved without a visit (F6, EM live test 2026-11-08): a cart of only
// packages is a sale, not a visit, so it used to show only in the ledger. Here it sits under the
// day's visits at Quầy, with the same columns and its own void ("Hủy hóa đơn"). The day's visit
// count stays a count of visits. Each sale shows its "Ghi chú gói" (F4) to health-note readers,
// and the desk edits it in place (`PATCH /sales/{id}`). Part of the lazy day-list chunk.
import type { DaySale } from "@vpyd/api-client";
import { queryOptions, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { formatVnd } from "@vpyd/ui";
import { useState } from "react";
import { CustomerName } from "../../app/CustomerName";
import { ResponsiveTable, type Column } from "../../components/ResponsiveTable";
import { RowMenu } from "../../components/RowMenu";
import { api } from "../../lib/api";
import { messageFor } from "../../lib/errors";
import { spaTime } from "../../lib/format";
import { invalidateManagementViews } from "../../lib/invalidate";
import { voidSale } from "../corrections/api";
import { VoidForm } from "../corrections/VoidForm";
import { SaleNoteForm } from "../sales/SaleNoteForm";

/**
 * Package sales of a business day saved without a visit (F6): shown under the day's visits at
 * Quầy, with their void. `date` undefined = today (the server's day).
 */
function daySalesQuery(date?: string) {
  return queryOptions({
    queryKey: ["sales", "day", date ?? "today"],
    queryFn: async () =>
      (await api(
        date ? `/sales?date=${encodeURIComponent(date)}` : "/sales",
      )) as DaySale[],
    staleTime: 15_000,
  });
}

type Payment = DaySale["sale"]["payments"][number];

/** "Tiền mặt" / "Chuyển khoản" / "Tiền mặt + CK" of the money taken; "" when none. */
export function methodText(payments: Pick<Payment, "kind" | "method">[]): string {
  const methods = new Set(payments.filter((p) => p.kind === "payment").map((p) => p.method));
  if (methods.size === 0) return "";
  if (methods.size > 1) return "Tiền mặt + CK";
  return methods.has("cash") ? "Tiền mặt" : "Chuyển khoản";
}

/** "Mua cho Nam · Người chốt: Lan", the line under the package names. */
function saleMeta(s: DaySale): string {
  return [
    s.beneficiary_name ? `Mua cho ${s.beneficiary_name}` : "",
    `Người chốt: ${s.sale.closer_name ?? "Khách tự chọn"}`,
  ]
    .filter(Boolean)
    .join(" · ");
}

const voided = (s: DaySale) => s.sale.status === "voided";

export function DaySales({
  day,
  isToday,
  mayVoid,
  mayEditNote,
  onNotice,
}: {
  day: string;
  isToday: boolean;
  /** UX only: the server decides (`403 day.locked` for a manager on an earlier day). */
  mayVoid: boolean;
  /**
   * UX only (`package.sell` + `customer.health_notes.write`, the closed-day rule of a void): the
   * server decides.
   */
  mayEditNote: boolean;
  onNotice: (text: string) => void;
}) {
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const sales = useQuery(daySalesQuery(isToday ? undefined : day));
  const [voiding, setVoiding] = useState<string | null>(null);
  const [noting, setNoting] = useState<string | null>(null);
  const rows = sales.data ?? [];
  if (sales.isError)
    return (
      <p role="alert" className="field-error">
        {messageFor(sales.error)}
      </p>
    );
  if (rows.length === 0) return null;

  const columns: Column<DaySale>[] = [
    {
      id: "time",
      header: "Giờ",
      width: "80px",
      card: "subtitle",
      className: "tabular-nums font-semibold whitespace-nowrap",
      cell: (s) => spaTime(s.sale.created_at),
    },
    {
      id: "customer",
      header: "Khách",
      width: "24%",
      card: "title",
      cell: (s) => (
        <span className="flex flex-wrap items-center gap-x-2 gap-y-1">
          <CustomerName name={s.customer_name} gender={s.customer_gender} />
          {voided(s) && <span className="badge badge-danger">Đã hủy</span>}
        </span>
      ),
    },
    {
      id: "packages",
      header: "Gói liệu trình",
      cardLabel: "Gói",
      cell: (s) => (
        <span className="grid min-w-0">
          {/* Whole names, wrapped: a clamp hid real package names ("Gội đầu dưỡng sinh 10 buổi + tặng 2…"). */}
          <span className="[overflow-wrap:anywhere]">{s.sale.lines.map((l) => l.description).join(", ")}</span>
          <span className="truncate text-text-muted">{saleMeta(s)}</span>
          {voided(s) && s.sale.void_reason && (
            <span className="truncate text-text-muted" title={s.sale.void_reason}>
              Lý do hủy: {s.sale.void_reason}
            </span>
          )}
          {s.sale.note && (
            <span className="line-clamp-3 whitespace-pre-line text-text-muted">
              Ghi chú gói: {s.sale.note}
            </span>
          )}
        </span>
      ),
    },
    {
      id: "payment",
      header: "Thanh toán",
      width: "16%",
      cell: (s) => (
        <span className="text-text-muted">
          {voided(s) ? "Đã hoàn tiền" : methodText(s.sale.payments) || "Chưa thu"}
        </span>
      ),
    },
    {
      id: "amount",
      header: "Số tiền",
      width: "112px",
      numeric: true,
      card: "aside",
      className: "visit-amount",
      cell: (s) => (voided(s) ? <s>{formatVnd(s.sale.total)}</s> : formatVnd(s.sale.total)),
    },
    {
      id: "actions",
      header: <span className="sr-only">Thao tác</span>,
      width: "60px",
      card: "actions",
      className: "text-right",
      cell: (s) => (
        <RowMenu
          label={`Thao tác hóa đơn ${spaTime(s.sale.created_at)} của ${s.customer_name}`}
          items={[
            {
              label: "Mở hồ sơ khách",
              onSelect: () => {
                void navigate({ to: "/khach-hang/$customerId", params: { customerId: s.sale.customer_id } });
              },
            },
            ...(mayEditNote && !voided(s)
              ? [
                  {
                    label: s.sale.note ? "Sửa ghi chú gói" : "Thêm ghi chú gói",
                    onSelect: () => {
                      setVoiding(null);
                      setNoting(s.sale.id);
                    },
                  },
                ]
              : []),
            ...(mayVoid && !voided(s)
              ? [
                  {
                    label: "Hủy hóa đơn",
                    danger: true,
                    onSelect: () => {
                      setNoting(null);
                      setVoiding(s.sale.id);
                    },
                  },
                ]
              : []),
          ]}
        />
      ),
    },
  ];

  return (
    <div className="grid gap-2">
      <h3 id="day-sales-title" className="section-label">
        Bán gói không kèm lượt ({String(rows.filter((s) => !voided(s)).length)})
      </h3>
      <div className="rt-host visits-host overflow-hidden rounded-md border border-border">
        <ResponsiveTable
          caption="Bán gói liệu trình không kèm lượt khách"
          rows={rows}
          rowKey={(s) => s.sale.id}
          cardsBelow={1024}
          rowClassName={(s) => (voided(s) ? "visit-voided" : undefined)}
          columns={columns}
          expanded={(s) =>
            voiding === s.sale.id ? (
              <VoidForm
                title={`Hủy hóa đơn bán ${s.sale.lines.map((l) => `"${l.description}"`).join(", ")} của ${s.customer_name}`}
                consequence="Liệu trình chưa dùng buổi nào sẽ bị hủy; tiền đã thu được ghi bút toán đảo (không xóa). Hóa đơn vẫn còn trong sổ với trạng thái đã hủy."
                confirmLabel="Hủy hóa đơn"
                run={(reason, key) => voidSale(s.sale.id, reason, key)}
                onCancel={() => {
                  setVoiding(null);
                }}
                onDone={() => {
                  setVoiding(null);
                  onNotice(`Đã hủy hóa đơn bán gói của ${s.customer_name}.`);
                  void queryClient.invalidateQueries({ queryKey: ["sales", "day"] });
                  void queryClient.invalidateQueries({ queryKey: ["customer-packages"] });
                  // Bought for someone else: their history changes too, and the row has no id for them.
                  void invalidateManagementViews(
                    queryClient,
                    s.beneficiary_name ? undefined : s.sale.customer_id,
                  );
                }}
              />
            ) : noting === s.sale.id ? (
              <SaleNoteForm
                saleId={s.sale.id}
                version={s.sale.version}
                note={s.sale.note}
                customerId={s.sale.customer_id}
                title={`Ghi chú gói ${s.sale.lines.map((l) => `"${l.description}"`).join(", ")} của ${s.customer_name}`}
                onCancel={() => {
                  setNoting(null);
                }}
                onDone={() => {
                  setNoting(null);
                  onNotice(`Đã lưu ghi chú gói của ${s.customer_name}.`);
                }}
              />
            ) : null
          }
        />
      </div>
    </div>
  );
}
