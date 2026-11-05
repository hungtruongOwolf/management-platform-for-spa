import { useInfiniteQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import type { ReactNode } from "react";
import { CustomerName } from "../../app/CustomerName";
import { NO_EXPIRY } from "../../lib/names";
import { useSearchState } from "../../lib/searchState";
import { FilterBar, SearchField } from "./filters";
import { packagesSearch } from "./search";
import { longDay, packagesOverviewQuery, relativeDay, type PackageFilter } from "./api";
import type { PackageOverviewRow, PackageStatus } from "./contract";
import { EmptyState, ErrorState, ManagementOnly, PageHeader, RowsSkeleton } from "./ui";

const STATUS: Record<PackageStatus, [string, string]> = {
  active: ["Đang dùng", "badge-ok"],
  expired: ["Hết hạn", "badge-warn"],
  exhausted: ["Hết buổi", "badge-muted"],
  cancelled: ["Đã hủy", "badge-danger"],
};

/** "+84905123456" → "0905 123 456" (design-system.md "Định dạng"); other shapes stay as they are. */
export function displayPhone(phone: string): string {
  const m = /^\+84(\d{3})(\d{3})(\d{3})$/.exec(phone);
  return m ? `0${m[1] ?? ""} ${m[2] ?? ""} ${m[3] ?? ""}` : phone;
}

/** Customer name linking to the profile with the gender beside it (EM round 3), phone below. */
export function CustomerCell({
  id,
  name,
  phone,
  gender,
}: {
  id: string;
  name: string;
  phone?: string;
  gender?: string | null;
}): ReactNode {
  return (
    <>
      <Link to="/khach-hang/$customerId" params={{ customerId: id }} className="font-semibold text-primary hover:underline">
        <CustomerName name={name} gender={gender} />
      </Link>
      {phone && <span className="mt-1 block text-xs text-text-muted tabular">{displayPhone(phone)}</span>}
    </>
  );
}

function Remaining({ p }: { p: PackageOverviewRow }) {
  const low = p.status === "active" && p.sessions_remaining <= 2;
  return (
    <span className="inline-flex flex-col items-end gap-1">
      <span className={`font-semibold tabular ${low ? "text-danger" : ""}`}>
        {p.sessions_remaining}/{p.sessions_total}
      </span>
      <span aria-hidden="true" className="split-bar block h-1.5 w-20">
        <span
          className={`split-part block ${low ? "bg-danger" : "tone-accent"}`}
          style={{ width: `${String(p.sessions_total ? (p.sessions_remaining / p.sessions_total) * 100 : 0)}%` }}
        />
      </span>
    </span>
  );
}

/** /goi-lieu-trinh — every customer package of the spa: what is running, expiring, finished. */
export function PackagesOverviewPage() {
  return (
    <ManagementOnly>
      <PackagesOverviewPageBody />
    </ManagementOnly>
  );
}

function PackagesOverviewPageBody() {
  const [{ status: filter, q }, patch] = useSearchState(packagesSearch);
  const page = useInfiniteQuery(packagesOverviewQuery(filter, q));
  const counts = page.data?.pages[0]?.counts;
  const items = page.data?.pages.flatMap((p) => p.items) ?? [];
  const filters: [PackageFilter, string, number | undefined][] = [
    ["expiring", "Sắp hết hạn", counts?.expiring_soon],
    ["active", "Đang dùng", counts?.active],
    ["expired", "Hết hạn", counts?.expired],
    ["exhausted", "Hết buổi", counts?.exhausted],
    ["cancelled", "Đã hủy", counts?.cancelled],
    ["all", "Tất cả", undefined],
  ];

  return (
    <section className="grid content-start gap-5">
      <PageHeader eyebrow="Gói liệu trình" title="Theo dõi gói của khách" />
      <FilterBar label="Tìm và lọc gói">
        <SearchField
          label="Tìm theo tên khách, tên gói hoặc số điện thoại"
          placeholder="Tên khách, tên gói, số điện thoại…"
          value={q}
          onCommit={(next) => {
            patch({ q: next });
          }}
        />
      </FilterBar>
      <div className="segmented" role="group" aria-label="Lọc theo trạng thái">
        {filters.map(([f, label, n]) => (
          <button
            key={f}
            type="button"
            aria-pressed={filter === f}
            onClick={() => {
              patch({ status: f });
            }}
          >
            {label}
            {n !== undefined && <span className="rounded-full bg-surface-2 px-2 text-xs tabular">{n}</span>}
          </button>
        ))}
      </div>
      <div className={`card overflow-hidden ${page.isPlaceholderData ? "is-refreshing" : ""}`}>
        {page.isError && (
          <div className="p-4">
            <ErrorState error={page.error} onRetry={() => void page.refetch()} />
          </div>
        )}
        {page.isPending && <RowsSkeleton rows={6} />}
        {page.isSuccess && items.length === 0 && (
          <EmptyState title="Không có gói nào">{q ? `Không thấy gói nào khớp “${q}”. Thử tên khác hoặc chọn “Tất cả”.` : "Thử chọn một trạng thái khác."}</EmptyState>
        )}
        {items.length > 0 && (
          <div className="table-wrap" tabIndex={0} role="region" aria-label="Danh sách gói liệu trình">
            <table className="data-table">
              <caption className="sr-only">Gói liệu trình của khách</caption>
              <thead>
                <tr>
                  <th scope="col">Khách</th>
                  <th scope="col">Gói</th>
                  <th scope="col" className="num">
                    Còn
                  </th>
                  <th scope="col">Hạn dùng</th>
                  <th scope="col">Dùng gần nhất</th>
                  <th scope="col">Trạng thái</th>
                </tr>
              </thead>
              <tbody>
                {items.map((p) => {
                  const [label, tone] = STATUS[p.status];
                  const soon = p.status === "active" && p.expires_on != null && p.days_to_expiry != null && p.days_to_expiry <= 30;
                  return (
                    <tr key={p.id}>
                      <td>
                        <CustomerCell id={p.customer_id} name={p.customer_name} phone={p.customer_phone} gender={p.customer_gender ?? null} />
                      </td>
                      <td>{p.name}</td>
                      <td className="num">
                        <Remaining p={p} />
                      </td>
                      <td className="tabular">
                        {p.expires_on ? longDay(p.expires_on) : <span className="badge badge-muted">{NO_EXPIRY}</span>}
                        {soon && <span className="block text-xs font-semibold text-warning-text">còn {p.days_to_expiry} ngày</span>}
                      </td>
                      <td className="text-text-muted">{p.last_used_on ? relativeDay(p.last_used_on) : "Chưa dùng"}</td>
                      <td>
                        <span className={`badge ${tone}`}>{label}</span>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </div>
      {page.hasNextPage && (
        <div className="flex justify-center">
          <button type="button" className="btn btn-secondary min-w-40" disabled={page.isFetchingNextPage} onClick={() => void page.fetchNextPage()}>
            {page.isFetchingNextPage ? "Đang tải…" : "Xem thêm"}
          </button>
        </div>
      )}
    </section>
  );
}

