import { useInfiniteQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import type { ReactNode } from "react";
import { CustomerName } from "../../app/CustomerName";
import { NO_EXPIRY } from "../../lib/names";
import { useSearchState } from "../../lib/searchState";
import { ResponsiveTable } from "../../components/ResponsiveTable";
import { spaToday } from "../corrections/api";
import { ENDING_DAYS, PACKAGE_STATE, packageState } from "../sales/packageState";
import { SearchField } from "./filters";
import { packagesSearch } from "./search";
import { longDay, packagesOverviewQuery, relativeDay, type PackageFilter } from "./api";
import type { PackageOverviewRow } from "./contract";
import { EmptyState, ErrorState, ManagementOnly, PageHeader, RowsSkeleton } from "./ui";

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

function Remaining({ p, ending }: { p: PackageOverviewRow; ending: boolean }) {
  return (
    <span className="inline-flex flex-col items-end gap-1">
      <span className={`font-semibold tabular ${ending ? "text-warning-text" : ""}`}>
        {p.sessions_remaining}/{p.sessions_total}
      </span>
      <span aria-hidden="true" className="split-bar block h-1.5 w-20">
        <span
          className={`split-part block ${ending ? "bg-warning-text" : "bg-primary"}`}
          style={{ width: `${String(p.sessions_total ? (p.sessions_remaining / p.sessions_total) * 100 : 0)}%` }}
        />
      </span>
    </span>
  );
}

/**
 * /goi-lieu-trinh — every customer package of the spa: what is running, expiring, finished. Layout
 * of round 5 (EM 2026-11-06 kept it); the status column shows the one shared package state
 * (sales/packageState.ts), "Sắp hết" included.
 */
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
  const today = spaToday();
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
      <PageHeader title="Gói liệu trình của khách" />
      <div role="search" aria-label="Tìm và lọc gói" className="grid content-start gap-3">
        <SearchField
          label="Tìm theo tên khách, tên gói hoặc số điện thoại"
          placeholder="Tên khách, tên gói, SĐT…"
          value={q}
          onCommit={(next) => {
            patch({ q: next });
          }}
        />
        <div className="segmented chip-row" role="group" aria-label="Lọc theo trạng thái">
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
      </div>
      <div className={`card rt-host overflow-hidden ${page.isPlaceholderData ? "is-refreshing" : ""}`}>
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
          <ResponsiveTable
            caption="Gói liệu trình của khách"
            rows={items}
            rowKey={(p) => p.id}
            stretchLink
            cardsBelow={1024}
            columns={[
              {
                id: "customer",
                header: "Khách",
                card: "title",
                cell: (p) => <CustomerCell id={p.customer_id} name={p.customer_name} phone={p.customer_phone} gender={p.customer_gender ?? null} />,
              },
              { id: "plan", header: "Gói", card: "subtitle", cell: (p) => p.name },
              {
                id: "left",
                header: "Còn",
                numeric: true,
                card: "aside",
                cell: (p) => <Remaining p={p} ending={packageState(p, today) === "ending"} />,
              },
              {
                id: "expiry",
                header: "Hạn dùng",
                className: "tabular",
                cell: (p) => {
                  const soon = p.status === "active" && p.expires_on != null && p.days_to_expiry != null && p.days_to_expiry <= ENDING_DAYS;
                  return (
                    <>
                      {p.expires_on ? longDay(p.expires_on) : <span className="badge badge-muted">{NO_EXPIRY}</span>}
                      {soon && <span className="block text-xs font-semibold text-warning-text">còn {p.days_to_expiry} ngày</span>}
                    </>
                  );
                },
              },
              { id: "used", header: "Dùng gần nhất", className: "text-text-muted", cell: (p) => (p.last_used_on ? relativeDay(p.last_used_on) : "Chưa dùng") },
              {
                id: "status",
                header: "Trạng thái",
                card: "aside",
                cell: (p) => {
                  const { label, badge } = PACKAGE_STATE[packageState(p, today)];
                  return <span className={`badge ${badge}`}>{label}</span>;
                },
              },
            ]}
          />
        )}
      </div>
      {page.hasNextPage && (
        <div className="flex justify-center">
          <button
            type="button"
            className="btn btn-secondary min-w-40"
            aria-busy={page.isFetchingNextPage || undefined}
            onClick={() => {
              if (!page.isFetchingNextPage) void page.fetchNextPage();
            }}
          >
            Xem thêm
          </button>
        </div>
      )}
    </section>
  );
}
