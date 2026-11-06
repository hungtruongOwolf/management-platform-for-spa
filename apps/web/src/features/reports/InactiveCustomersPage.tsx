import type { FollowUpFilter, InactiveCustomer } from "@vpyd/api-client";
import { useInfiniteQuery, useSuspenseQuery } from "@tanstack/react-query";
import { useState } from "react";
import { Icon } from "../../app/Icon";
import { useSearchState } from "../../lib/searchState";
import { can, meQuery } from "../auth/session";
import { spaToday } from "../corrections/api";
import { CHANNEL_LABEL, OUTCOME_LABEL } from "../customers/followUps";
import { FollowUpForm } from "../customers/FollowUpForm";
import { inactiveCustomersQuery, relativeDay, shortDay, type InactiveFilter } from "./api";
import { BottomSheet } from "../../components/BottomSheet";
import { ResponsiveTable, type Column } from "../../components/ResponsiveTable";
import { FilterSelect, ResponsiveFilters, SearchField } from "./filters";
import { CustomerCell } from "./PackagesOverviewPage";
import { THRESHOLDS, inactiveSearch } from "./search";
import { EmptyState, ErrorState, ManagementOnly, PageHeader, RowsSkeleton } from "./ui";

const FOLLOW_UPS: [FollowUpFilter, string][] = [
  ["due", "Cần liên hệ"],
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
  const [search, patch] = useSearchState(inactiveSearch);
  const filter: InactiveFilter = { days: search.days, scope: search.scope, kind: search.kind, followUp: search.follow, q: search.q };
  const page = useInfiniteQuery(inactiveCustomersQuery(filter));
  const items = page.data?.pages.flatMap((p) => p.items) ?? [];
  const canFollowUp = can(me, "customer.follow_up");
  const [contacting, setContacting] = useState<InactiveCustomer | null>(null);
  const [notice, setNotice] = useState("");

  return (
    <section className="grid content-start gap-5">
      <PageHeader title="Khách lâu chưa quay lại" />
      <p className="max-w-2xl text-text-muted">
        Khách liệu trình và khách lẻ chưa quay lại từ {filter.days} ngày trở lên. “Cần liên hệ” xếp người chưa gọi và người đến hạn gọi lại lên trước; liên hệ xong bấm “Đã liên hệ” để hẹn lần sau.
      </p>
      <ResponsiveFilters
        label="Lọc khách vắng"
        activeCount={[filter.days !== THRESHOLDS[0], filter.kind !== "all", filter.scope !== "all"].filter(Boolean).length}
        onReset={() => {
          patch({ days: THRESHOLDS[0], kind: "all", scope: "all" });
        }}
        search={
          <SearchField
            label="Tìm theo tên hoặc số điện thoại"
            placeholder="Tên khách, SĐT…"
            value={filter.q}
            onCommit={(q) => {
              patch({ q });
            }}
          />
        }
      >
        <FilterSelect
          label="Vắng từ"
          value={String(filter.days)}
          options={THRESHOLDS.map((d) => [String(d), `${String(d)} ngày`] as const)}
          onChange={(d) => {
            patch({ days: Number(d) as (typeof THRESHOLDS)[number] });
          }}
        />
        <FilterSelect
          label="Loại khách"
          value={filter.kind}
          options={KINDS}
          onChange={(kind) => {
            patch({ kind });
          }}
        />
        <FilterSelect
          label="Phạm vi"
          value={filter.scope}
          options={SCOPES}
          onChange={(scope) => {
            patch({ scope });
          }}
        />
      </ResponsiveFilters>
      <div className="segmented chip-row" role="group" aria-label="Tình trạng liên hệ">
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
      <div className={`card rt-host overflow-hidden ${page.isPlaceholderData ? "is-refreshing" : ""}`}>
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
          <ResponsiveTable
            caption="Khách lâu chưa quay lại"
            rows={items}
            rowKey={(c) => c.customer_id}
            stretchLink
            cardsBelow={1024}
            columns={inactiveColumns(canFollowUp, setContacting)}
          />
        )}
      </div>
      {page.hasNextPage && (
        <div className="flex justify-center">
          <button type="button" className="btn btn-secondary min-w-40" disabled={page.isFetchingNextPage} onClick={() => void page.fetchNextPage()}>
            {page.isFetchingNextPage ? "Đang tải…" : "Xem thêm"}
          </button>
        </div>
      )}
      <BottomSheet
        open={contacting !== null}
        onClose={() => {
          setContacting(null);
        }}
        title="Đã liên hệ"
        description={contacting?.full_name}
      >
        {contacting && (
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
        )}
      </BottomSheet>
    </section>
  );
}

function NextBadge({ on, today }: { on: string; today: string }) {
  const due = on <= today;
  return (
    <span className={`badge ${due ? "badge-danger" : "badge-ok"}`}>
      {due ? "Đến hạn" : "Hẹn"} {relativeDay(on)}
    </span>
  );
}

// EM round 5 item 8: no call button for now (the desk calls from its own phone); marking the
// contact is the one action. The number stays readable under the name.
function MarkContacted({ c, onContacted }: { c: InactiveCustomer; onContacted: () => void }) {
  return (
    <button type="button" className="btn btn-secondary whitespace-nowrap" onClick={onContacted} aria-label={`Đã liên hệ với ${c.full_name}`}>
      <Icon name="check" className="size-4" /> Đã liên hệ
    </button>
  );
}

/** Columns of the absent-customer list: a table from 768 px, cards with the actions below that. */
function inactiveColumns(canFollowUp: boolean, onContacted: (c: InactiveCustomer) => void): Column<InactiveCustomer>[] {
  const today = spaToday();
  return [
    {
      id: "customer",
      header: "Khách",
      card: "title",
      cell: (c) => (
        <>
          <CustomerCell id={c.customer_id} name={c.full_name} phone={c.phone} gender={c.gender ?? null} />
          <span className={`badge mt-1 ${c.kind === "package" ? "badge-gold" : "badge-muted"}`}>
            {c.kind === "package" ? `Liệu trình · còn ${String(c.sessions_remaining)} buổi` : "Khách lẻ"}
          </span>
        </>
      ),
    },
    {
      id: "days",
      header: "Số ngày vắng",
      numeric: true,
      card: "aside",
      cell: (c) => <span className={`badge ${c.days_absent >= 30 ? "badge-danger" : "badge-warn"}`}>{c.days_absent} ngày</span>,
    },
    {
      id: "last",
      header: "Lần cuối",
      className: "tabular",
      cell: (c) => (
        <>
          {relativeDay(c.last_seen_on)}
          {!c.last_visit_on && <span className="block text-xs text-text-muted">Chưa đến lần nào</span>}
        </>
      ),
    },
    {
      id: "contact",
      header: "Liên hệ gần nhất",
      cell: (c) => {
        const f = c.last_follow_up;
        return f ? (
          <>
            <span className="tabular">{relativeDay(f.on)}</span>
            <span className="block text-xs text-text-muted">
              {CHANNEL_LABEL[f.channel]} · {OUTCOME_LABEL[f.outcome]} · {f.by}
            </span>
          </>
        ) : (
          <span className="text-text-muted">Chưa liên hệ</span>
        );
      },
    },
    {
      id: "next",
      header: "Hẹn lại",
      className: "tabular",
      cardOmit: (c) => !c.next_follow_up_on,
      cell: (c) => (c.next_follow_up_on ? <NextBadge on={c.next_follow_up_on} today={today} /> : <span className="text-text-muted">—</span>),
    },
    // Without customer.follow_up the row has nothing to do: no empty actions column.
    ...(canFollowUp
      ? [
          {
            id: "actions",
            header: <span className="sr-only">Thao tác</span>,
            card: "actions" as const,
            cell: (c: InactiveCustomer) => (
              <span className="flex justify-end">
                <MarkContacted
                  c={c}
                  onContacted={() => {
                    onContacted(c);
                  }}
                />
              </span>
            ),
          },
        ]
      : []),
  ];
}
