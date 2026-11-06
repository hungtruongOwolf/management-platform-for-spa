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
import "../customers/khach.css";

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

/** /goi-lieu-trinh — every customer package of the spa: what is running, expiring, finished. */
export function PackagesOverviewPage() {
  return (
    <ManagementOnly>
      <PackagesOverviewPageBody />
    </ManagementOnly>
  );
}

function StateChip({ p, today }: { p: PackageOverviewRow; today: string }) {
  const { label, badge } = PACKAGE_STATE[packageState(p, today)];
  return <span className={`badge ${badge}`}>{label}</span>;
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
    <section className="khach-page">
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
          <EmptyState title="Không có gói nào">
            {q
              ? `Không thấy gói nào khớp “${q}”. Thử tên khác hoặc chọn “Tất cả”.`
              : filter === "all"
                ? "Chưa bán gói liệu trình nào. Bán gói từ hồ sơ khách: “Bán gói liệu trình”."
                : "Thử chọn một trạng thái khác, hoặc “Tất cả”."}
          </EmptyState>
        )}
        {items.length > 0 && (
          <ResponsiveTable
            caption="Gói liệu trình của khách"
            rows={items}
            rowKey={(p) => p.id}
            stretchLink
            cardsBelow={1024}
            rowClassName={(p) => (p.status === "cancelled" || p.status === "exhausted" ? "visit-voided" : undefined)}
            columns={[
              {
                id: "customer",
                header: "Khách",
                card: "title",
                cell: (p) => <CustomerCell id={p.customer_id} name={p.customer_name} phone={p.customer_phone} gender={p.customer_gender ?? null} />,
              },
              { id: "plan", header: "Gói", card: "subtitle", width: "26%", cell: (p) => p.name },
              {
                id: "left",
                header: "Còn",
                numeric: true,
                card: "aside",
                width: "88px",
                cell: (p) => (
                  <span className="font-semibold">
                    {p.sessions_remaining}/{p.sessions_total}
                  </span>
                ),
              },
              {
                id: "expiry",
                header: "Hạn dùng",
                className: "tabular",
                width: "136px",
                cell: (p) => {
                  const soon = p.status === "active" && p.expires_on != null && p.days_to_expiry != null && p.days_to_expiry <= ENDING_DAYS;
                  return (
                    <>
                      {p.expires_on ? longDay(p.expires_on) : NO_EXPIRY}
                      {soon && <span className="cell-note">còn {p.days_to_expiry} ngày</span>}
                    </>
                  );
                },
              },
              {
                id: "used",
                header: "Dùng gần nhất",
                className: "text-text-muted",
                width: "128px",
                cell: (p) => (p.last_used_on ? relativeDay(p.last_used_on) : "Chưa dùng"),
              },
              {
                id: "status",
                header: "Trạng thái",
                card: "aside",
                width: "112px",
                cell: (p) => <StateChip p={p} today={today} />,
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
            Xem thêm gói
          </button>
        </div>
      )}
    </section>
  );
}
