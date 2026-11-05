import type { InactiveScope } from "@vpyd/api-client";
import { useInfiniteQuery } from "@tanstack/react-query";
import { useState } from "react";
import { inactiveCustomersQuery, relativeDay } from "./api";
import { CustomerCell } from "./PackagesOverviewPage";
import { EmptyState, ErrorState, ManagementOnly, PageHeader, RowsSkeleton } from "./ui";

/** Absence thresholds offered (EM 2026-11-05: 7 days is the default). */
export const THRESHOLDS = [7, 14, 30] as const;

const SCOPES: [InactiveScope, string][] = [
  ["with_sessions", "Còn buổi trong gói"],
  ["all", "Mọi khách đã từng đến"],
];

/** /khach-lau-khong-quay-lai — every overdue customer, longest absence first: the list to call. */
export function InactiveCustomersPage() {
  return (
    <ManagementOnly>
      <InactiveCustomersPageBody />
    </ManagementOnly>
  );
}

function InactiveCustomersPageBody() {
  const [days, setDays] = useState<number>(THRESHOLDS[0]);
  const [scope, setScope] = useState<InactiveScope>("with_sessions");
  const page = useInfiniteQuery(inactiveCustomersQuery(days, scope));
  const items = page.data?.pages.flatMap((p) => p.items) ?? [];

  return (
    <section className="grid gap-5">
      <PageHeader eyebrow="Khách hàng" title="Khách lâu không quay lại" />
      <p className="max-w-2xl text-text-muted">
        Khách chưa quay lại spa từ {days} ngày trở lên, vắng lâu nhất ở trên. Gọi nhắc lịch để khách dùng tiếp liệu trình.
      </p>
      <div className="flex flex-wrap items-center gap-3">
        <div className="segmented" role="group" aria-label="Số ngày chưa đến">
          {THRESHOLDS.map((d) => (
            <button
              key={d}
              type="button"
              aria-pressed={days === d}
              onClick={() => {
                setDays(d);
              }}
            >
              ≥ {d} ngày
            </button>
          ))}
        </div>
        <div className="segmented" role="group" aria-label="Phạm vi">
          {SCOPES.map(([s, label]) => (
            <button
              key={s}
              type="button"
              aria-pressed={scope === s}
              onClick={() => {
                setScope(s);
              }}
            >
              {label}
            </button>
          ))}
        </div>
      </div>
      <div className="card overflow-hidden">
        {page.isError && (
          <div className="p-4">
            <ErrorState error={page.error} onRetry={() => void page.refetch()} />
          </div>
        )}
        {page.isPending && <RowsSkeleton rows={6} />}
        {page.isSuccess && items.length === 0 && (
          <EmptyState title="Không có khách nào vắng lâu">Mọi khách trong phạm vi này đã quay lại trong {days} ngày qua.</EmptyState>
        )}
        {items.length > 0 && (
          <div className="table-wrap">
            <table className="data-table">
              <caption className="sr-only">Khách lâu không quay lại</caption>
              <thead>
                <tr>
                  <th scope="col">Khách</th>
                  <th scope="col" className="num">
                    Số ngày vắng
                  </th>
                  <th scope="col">Lần cuối đến</th>
                  <th scope="col" className="num">
                    Gói đang dùng
                  </th>
                  <th scope="col" className="num">
                    Buổi còn lại
                  </th>
                </tr>
              </thead>
              <tbody>
                {items.map((c) => (
                  <tr key={c.customer_id}>
                    <td>
                      <CustomerCell id={c.customer_id} name={c.full_name} phone={c.phone} />
                    </td>
                    <td className="num">
                      <span className={`badge ${c.days_absent >= 30 ? "badge-danger" : "badge-warn"}`}>{c.days_absent} ngày</span>
                    </td>
                    <td className="tabular">
                      {c.last_visit_on ? relativeDay(c.last_visit_on) : <span className="text-text-muted">Chưa đến lần nào</span>}
                    </td>
                    <td className="num">{c.active_packages}</td>
                    <td className="num font-semibold">{c.sessions_remaining}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
      {page.hasNextPage && (
        <div className="flex justify-center">
          <button
            type="button"
            className="btn btn-secondary min-w-40"
            disabled={page.isFetchingNextPage}
            onClick={() => void page.fetchNextPage()}
          >
            {page.isFetchingNextPage ? "Đang tải…" : "Xem thêm"}
          </button>
        </div>
      )}
    </section>
  );
}
