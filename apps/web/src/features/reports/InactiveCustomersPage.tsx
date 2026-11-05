import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { inactiveCustomersQuery, relativeDay } from "./api";
import { CustomerCell } from "./PackagesOverviewPage";
import { EmptyState, ErrorState, ManagementOnly, MockBanner, PageHeader, RowsSkeleton } from "./ui";

const WINDOWS = [14, 30, 60, 90] as const;

/** /nhac-khach — customers with sessions left who have not come back: the list to call. */
export function InactiveCustomersPage() {
  return (
    <ManagementOnly>
      <InactiveCustomersPageBody />
    </ManagementOnly>
  );
}

function InactiveCustomersPageBody() {
  const [days, setDays] = useState<number>(30);
  const page = useQuery(inactiveCustomersQuery(days));
  return (
    <section className="space-y-5">
      <PageHeader eyebrow="Khách hàng" title="Lâu chưa quay lại">
        <div className="segmented" role="group" aria-label="Số ngày chưa đến">
          {WINDOWS.map((d) => (
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
      </PageHeader>
      <p className="max-w-2xl text-text-muted">Khách còn buổi trong gói đang dùng nhưng chưa quay lại spa. Gọi nhắc lịch để khách dùng hết liệu trình.</p>
      <MockBanner />
      <div className="card overflow-hidden">
        {page.isError && (
          <div className="p-4">
            <ErrorState error={page.error} onRetry={() => void page.refetch()} />
          </div>
        )}
        {page.isPending && <RowsSkeleton rows={6} />}
        {page.data?.items.length === 0 && (
          <EmptyState title="Không có khách nào vắng lâu">Mọi khách còn buổi đều đã quay lại trong {days} ngày qua.</EmptyState>
        )}
        {page.data && page.data.items.length > 0 && (
          <div className="overflow-x-auto">
            <table className="data-table">
              <caption className="sr-only">Khách lâu chưa quay lại</caption>
              <thead>
                <tr>
                  <th scope="col">Khách</th>
                  <th scope="col">Lần cuối</th>
                  <th scope="col" className="num">
                    Số ngày vắng
                  </th>
                  <th scope="col" className="num">
                    Gói đang dùng
                  </th>
                  <th scope="col" className="num">
                    Buổi còn lại
                  </th>
                </tr>
              </thead>
              <tbody>
                {page.data.items.map((c) => (
                  <tr key={c.customer_id}>
                    <td>
                      <CustomerCell id={c.customer_id} name={c.full_name} phone={c.phone} />
                    </td>
                    <td className="tabular">{relativeDay(c.last_seen_on)}</td>
                    <td className="num">
                      <span className={`badge ${c.days_absent >= 60 ? "badge-danger" : "badge-warn"}`}>{c.days_absent} ngày</span>
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
    </section>
  );
}
