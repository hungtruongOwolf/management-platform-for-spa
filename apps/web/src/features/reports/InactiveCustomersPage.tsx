import type { FollowUpFilter, InactiveCustomer } from "@vpyd/api-client";
import { useInfiniteQuery, useSuspenseQuery } from "@tanstack/react-query";
import { useNavigate, useSearch } from "@tanstack/react-router";
import { useState } from "react";
import { Modal } from "../../app/AppShell";
import { Icon } from "../../app/Icon";
import { can, meQuery } from "../auth/session";
import { spaToday } from "../corrections/api";
import { CHANNEL_LABEL, OUTCOME_LABEL } from "../customers/followUps";
import { FollowUpForm } from "../customers/FollowUpForm";
import { inactiveCustomersQuery, relativeDay, shortDay, type InactiveFilter } from "./api";
import { FilterBar, FilterSelect, SearchField } from "./filters";
import { CustomerCell } from "./PackagesOverviewPage";
import { INACTIVE_THRESHOLDS, parseInactiveSearch } from "./search";
import { EmptyState, ErrorState, ManagementOnly, PageHeader, RowsSkeleton } from "./ui";

const FOLLOW_UPS: [FollowUpFilter, string][] = [
  ["due", "Cần liên hệ"],
  ["not_contacted", "Chưa liên hệ"],
  ["scheduled", "Đã hẹn"],
  ["all", "Tất cả"],
];

const KINDS = [
  ["all", "Mọi khách"],
  ["package", "Khách liệu trình"],
  ["one_off", "Khách lẻ"],
] as const;

const SCOPES = [
  ["all", "Mọi khách đã từng đến"],
  ["with_sessions", "Chỉ khách còn buổi"],
] as const;

const EMPTY: Record<FollowUpFilter, string> = {
  due: "Không còn ai cần liên hệ hôm nay.",
  not_contacted: "Khách vắng lâu nào cũng đã được liên hệ.",
  scheduled: "Chưa hẹn liên hệ lại với khách nào.",
  all: "Mọi khách đã quay lại trong khoảng này.",
};

/**
 * /khach-lau-khong-quay-lai: package and one-off customers who have not come back (EM round 3
 * item 4), longest absence first, with the follow-up state (item 16): last contact, next try,
 * filters "Cần liên hệ / Chưa liên hệ / Đã hẹn", and "Đã liên hệ" to record a contact. Every
 * filter lives in the URL, so back from a profile returns to the same list.
 */
export function InactiveCustomersPage() {
  return (
    <ManagementOnly>
      <InactiveCustomersPageBody />
    </ManagementOnly>
  );
}

function InactiveCustomersPageBody() {
  const { data: me } = useSuspenseQuery(meQuery);
  const search = useSearch({ from: "/authed/khach-lau-khong-quay-lai" });
  const navigate = useNavigate({ from: "/khach-lau-khong-quay-lai" });
  const filter: InactiveFilter = {
    days: search.days ?? 7,
    scope: search.scope ?? "all",
    kind: search.kind ?? "all",
    followUp: search.follow ?? "due",
    q: search.q ?? "",
  };
  const patch = (p: Record<string, string | number | undefined>) =>
    void navigate({ search: (prev) => parseInactiveSearch({ ...prev, ...p }), replace: true });
  const page = useInfiniteQuery(inactiveCustomersQuery(filter));
  const items = page.data?.pages.flatMap((p) => p.items) ?? [];
  const canFollowUp = can(me, "customer.follow_up");
  const [contacting, setContacting] = useState<InactiveCustomer | null>(null);
  const [notice, setNotice] = useState("");

  return (
    <section className="grid content-start gap-5">
      <PageHeader eyebrow="Khách hàng" title="Khách lâu không quay lại" />
      <p className="max-w-2xl text-text-muted">
        Khách liệu trình và khách lẻ chưa quay lại từ {filter.days} ngày trở lên, vắng lâu nhất ở trên. Gọi nhắc lịch rồi bấm “Đã liên hệ” để biết khi nào gọi lại.
      </p>
      <FilterBar label="Tìm và lọc khách">
        <SearchField
          label="Tìm theo tên hoặc số điện thoại"
          placeholder="Tên khách, số điện thoại…"
          value={filter.q}
          onCommit={(q) => {
            patch({ q: q || undefined });
          }}
        />
        <FilterSelect
          label="Vắng từ"
          value={String(filter.days)}
          options={INACTIVE_THRESHOLDS.map((d) => [String(d), `${String(d)} ngày`] as const)}
          onChange={(d) => {
            patch({ days: Number(d) });
          }}
        />
        <FilterSelect
          label="Loại khách"
          value={filter.kind}
          options={KINDS}
          onChange={(kind) => {
            patch({ kind: kind === "all" ? undefined : kind });
          }}
        />
        <FilterSelect
          label="Phạm vi"
          value={filter.scope}
          options={SCOPES}
          onChange={(scope) => {
            patch({ scope: scope === "all" ? undefined : scope });
          }}
        />
      </FilterBar>
      <div className="segmented" role="group" aria-label="Tình trạng liên hệ">
        {FOLLOW_UPS.map(([f, label]) => (
          <button
            key={f}
            type="button"
            aria-pressed={filter.followUp === f}
            onClick={() => {
              patch({ follow: f });
            }}
          >
            {label}
          </button>
        ))}
      </div>
      <p role="status" className="sr-only">
        {notice}
      </p>
      <div className={`card overflow-hidden ${page.isPlaceholderData ? "is-refreshing" : ""}`}>
        {page.isError && (
          <div className="p-4">
            <ErrorState error={page.error} onRetry={() => void page.refetch()} />
          </div>
        )}
        {page.isPending && <RowsSkeleton rows={6} />}
        {page.isSuccess && items.length === 0 && (
          <EmptyState title="Không có khách nào">{filter.q ? `Không thấy khách nào khớp “${filter.q}”.` : EMPTY[filter.followUp]}</EmptyState>
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
                  <th scope="col">Lần cuối</th>
                  <th scope="col">Liên hệ gần nhất</th>
                  <th scope="col">Hẹn lại</th>
                  <th scope="col">
                    <span className="sr-only">Thao tác</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {items.map((c) => (
                  <InactiveRow
                    key={c.customer_id}
                    c={c}
                    canFollowUp={canFollowUp}
                    onContacted={() => {
                      setContacting(c);
                    }}
                  />
                ))}
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
      <Modal
        open={contacting !== null}
        onClose={() => {
          setContacting(null);
        }}
        className="form-dialog"
        label={contacting ? `Đã liên hệ với ${contacting.full_name}` : "Đã liên hệ"}
      >
        {contacting && (
          <div className="grid gap-4">
            <h2 className="font-brand text-xl text-primary">Đã liên hệ với {contacting.full_name}</h2>
            <FollowUpForm
              customerId={contacting.customer_id}
              customerName={contacting.full_name}
              onCancel={() => {
                setContacting(null);
              }}
              onDone={(saved) => {
                setNotice(
                  `Đã ghi liên hệ với ${contacting.full_name}${saved.next_follow_up_on ? `, hẹn lại ${shortDay(saved.next_follow_up_on)}` : ""}.`,
                );
                setContacting(null);
              }}
            />
          </div>
        )}
      </Modal>
    </section>
  );
}

function InactiveRow({ c, canFollowUp, onContacted }: { c: InactiveCustomer; canFollowUp: boolean; onContacted: () => void }) {
  const today = spaToday();
  const due = c.next_follow_up_on != null && c.next_follow_up_on <= today;
  const f = c.last_follow_up;
  return (
    <tr>
      <td>
        <CustomerCell id={c.customer_id} name={c.full_name} phone={c.phone} gender={c.gender ?? null} />
        <span className={`badge mt-1 ${c.kind === "package" ? "badge-gold" : "badge-muted"}`}>
          {c.kind === "package" ? `Liệu trình · còn ${String(c.sessions_remaining)} buổi` : "Khách lẻ"}
        </span>
      </td>
      <td className="num">
        <span className={`badge ${c.days_absent >= 30 ? "badge-danger" : "badge-warn"}`}>{c.days_absent} ngày</span>
      </td>
      <td className="tabular">
        {relativeDay(c.last_seen_on)}
        {!c.last_visit_on && <span className="block text-xs text-text-muted">Chưa đến lần nào</span>}
      </td>
      <td>
        {f ? (
          <>
            <span className="tabular">{relativeDay(f.on)}</span>
            <span className="block text-xs text-text-muted">
              {CHANNEL_LABEL[f.channel]} · {OUTCOME_LABEL[f.outcome]} · {f.by}
            </span>
          </>
        ) : (
          <span className="text-text-muted">Chưa liên hệ</span>
        )}
      </td>
      <td className="tabular">
        {c.next_follow_up_on ? (
          <span className={`badge ${due ? "badge-danger" : "badge-ok"}`}>
            {due ? "Đến hạn" : "Hẹn"} {relativeDay(c.next_follow_up_on)}
          </span>
        ) : (
          <span className="text-text-muted">—</span>
        )}
      </td>
      <td>
        <span className="flex flex-wrap justify-end gap-2">
          <a className="icon-btn" href={`tel:${c.phone.replace(/[^\d+]/g, "")}`} aria-label={`Gọi ${c.full_name}`} title="Gọi điện">
            <Icon name="phone" />
          </a>
          {canFollowUp && (
            <button type="button" className="btn btn-secondary whitespace-nowrap" onClick={onContacted} aria-label={`Đã liên hệ với ${c.full_name}`}>
              <Icon name="check" className="size-4" /> Đã liên hệ
            </button>
          )}
        </span>
      </td>
    </tr>
  );
}
