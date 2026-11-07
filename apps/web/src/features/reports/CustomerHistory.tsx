import type { HistoryKind } from "@vpyd/api-client";
import { useInfiniteQuery, useQuery, type QueryClient } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { useState } from "react";
import { canSeeManagement } from "../../app/features";
import { Icon, type IconName } from "../../app/Icon";
import { monthLabel } from "../../lib/format";
import { serviceLabel } from "../../lib/names";
import { customerPackagesQuery, formatDate } from "../sales/api";
import { useSearchState } from "../../lib/searchState";
import { authQuery, can, type Me } from "../auth/session";
import { mayCorrectVisit, spaToday } from "../corrections/api";
import { customerQuery } from "../customers/api";
import { CHANNEL_LABEL, OUTCOME_LABEL, followUpsQuery } from "../customers/followUps";
import { FollowUpForm } from "../customers/FollowUpForm";
import { HISTORY_KINDS, historySearch } from "../customers/search";
import { customerHistoryQuery, customerStatsQuery, relativeDay, type HistoryFilter } from "./api";
import type { HistoryEvent } from "./contract";
import { BottomSheet } from "../../components/BottomSheet";
import { DateField } from "../../components/DateField";
import { FilterSelect, ResponsiveFilters, SearchField } from "./filters";
import { EmptyState, ErrorState, RowsSkeleton, money } from "./ui";
import "../customers/khach.css";

const time = new Intl.DateTimeFormat("vi-VN", { hour: "2-digit", minute: "2-digit", timeZone: "Asia/Ho_Chi_Minh" });

/** Names of the history kinds (the "Loại" filter). */
const KIND: Record<HistoryKind, string> = {
  visit: "Lượt khách",
  purchase: "Mua gói liệu trình",
  visit_voided: "Lượt đã hủy",
  purchase_voided: "Hóa đơn đã hủy",
  package_cancelled: "Hủy gói",
  package_extended: "Gia hạn gói",
};

const KIND_OPTIONS: readonly (readonly [HistoryKind | "", string])[] = [["", "Mọi loại"], ...HISTORY_KINDS.map((k) => [k, KIND[k]] as const)];

/** Title and the one state chip of a timeline row (design-system.md "Quy tắc" 7). */
function eventHead(e: HistoryEvent): { title: string; chip?: [string, string] } {
  switch (e.kind) {
    case "visit":
      return e.visit?.corrects_visit_id ? { title: "Lượt khách", chip: ["Lượt sửa lại", "badge-info"] } : { title: "Lượt khách" };
    case "visit_voided":
      return e.visit?.corrected_by_visit_id
        ? { title: "Lượt khách", chip: ["Đã sửa", "badge-muted"] }
        : { title: "Lượt khách", chip: ["Đã hủy", "badge-danger"] };
    case "purchase":
      return { title: "Mua gói liệu trình" };
    case "purchase_voided":
      return { title: "Mua gói liệu trình", chip: ["Đã hủy", "badge-danger"] };
    case "package_cancelled":
      return { title: "Hủy gói", chip: ["Đã hủy", "badge-danger"] };
    case "package_extended":
      return { title: "Gia hạn gói" };
  }
}

/** Dot colour and icon of each kind on the timeline (round-5 layout, kept by the EM 2026-11-06). */
const DOT: Record<HistoryKind, [IconName, string]> = {
  visit: ["desk", "bg-primary"],
  purchase: ["package", "bg-accent"],
  visit_voided: ["x", "bg-border-strong"],
  purchase_voided: ["x", "bg-border-strong"],
  package_cancelled: ["alert", "bg-danger"],
  package_extended: ["calendar", "bg-accent"],
};

function Fact({ label, value, hint }: { label: string; value: string; hint?: string | undefined }) {
  return (
    <div className="rounded-md bg-surface-2 p-3">
      <dt className="text-sm text-text-muted">{label}</dt>
      <dd className="mt-0.5 text-lg font-semibold tabular">{value}</dd>
      {hint && <dd className="text-xs text-text-muted">{hint}</dd>}
    </div>
  );
}

function Summary({ customerId }: { customerId: string }) {
  const stats = useQuery(customerStatsQuery(customerId));
  if (stats.isPending)
    return (
      <div role="status" aria-label="Đang tải tóm tắt" className="grid grid-cols-2 gap-3">
        {[0, 1, 2, 3].map((i) => (
          <div key={i} className="skeleton h-[72px]" />
        ))}
      </div>
    );
  if (stats.isError) return <ErrorState error={stats.error} onRetry={() => void stats.refetch()} />;
  const s = stats.data;
  return (
    <dl className="grid grid-cols-2 gap-3">
      <Fact
        label="Lần gần nhất"
        value={s.last_visit_on ? relativeDay(s.last_visit_on) : "Chưa đến"}
        hint={s.days_since_last_visit != null && s.days_since_last_visit > 1 ? `${String(s.days_since_last_visit)} ngày trước` : undefined}
      />
      <Fact label="Số lượt" value={String(s.visit_count)} hint={s.first_visit_on ? `từ ${relativeDay(s.first_visit_on)}` : undefined} />
      <Fact label="Buổi còn lại" value={String(s.sessions_remaining)} hint={`${String(s.packages.active)} gói đang dùng`} />
      {/* One number: migrated paper-book purchases are booked as ordinary sales (EM round-4 answers). */}
      <Fact label="Đã chi" value={money(s.net_spent)} hint={s.refunded ? `đã hoàn ${money(s.refunded)}` : undefined} />
    </dl>
  );
}

function EventBody({ e }: { e: HistoryEvent }) {
  if (e.visit) {
    const v = e.visit;
    return (
      <ul className="mt-1 grid content-start gap-1 text-sm">
        {v.lines.map((l, i) => (
          <li key={i} className="flex items-baseline justify-between gap-x-3">
            <span className="min-w-0">
              {serviceLabel(l.service_name, l.category_name)}
              <span className="text-text-muted">
                {l.source === "package" ? ` · trừ buổi${l.sessions_left_after != null ? `, còn ${String(l.sessions_left_after)}` : ""}` : " · dịch vụ lẻ"} · KTV {l.staff_name}
              </span>
            </span>
            {l.amount > 0 && (
              <span className={`shrink-0 whitespace-nowrap tabular ${v.voided ? "line-through" : ""}`}>{money(l.amount)}</span>
            )}
          </li>
        ))}
        {v.corrected_by_visit_id ? (
          <li className="text-text-muted">Đã sửa thành lượt mới · {v.void_reason}</li>
        ) : (
          v.void_reason && <li className="text-text-muted">Lý do hủy: {v.void_reason}</li>
        )}
        {v.corrects_visit_id && <li className="text-text-muted">Thay cho lượt ghi trước (đã sửa).</li>}
        {v.note && (
          <li className="whitespace-pre-line">
            <span className="text-text-muted">Ghi chú buổi: </span>
            {v.note}
          </li>
        )}
      </ul>
    );
  }
  if (e.purchase) {
    return (
      <div className="mt-1 text-sm">
        {e.purchase.lines.map((l, i) => (
          <p key={i}>{l.description}</p>
        ))}
        {/* Buy-for-other (US13): the payer sees who uses it; the user sees who paid. */}
        {e.purchase.payer_name ? (
          <p>{e.purchase.payer_name} mua tặng</p>
        ) : (
          e.purchase.beneficiary_name && <p>Mua cho {e.purchase.beneficiary_name}</p>
        )}
        <p className="text-text-muted">
          Thanh toán <span className={`font-semibold text-text tabular ${e.purchase.voided ? "line-through" : ""}`}>{money(e.purchase.total)}</span>
          {e.purchase.discount_amount > 0 && ` (giảm ${money(e.purchase.discount_amount)})`}
          {e.purchase.closer_name && ` · chốt bởi ${e.purchase.closer_name}`}
        </p>
        {e.purchase.void_reason && <p className="text-text-muted">Lý do hủy: {e.purchase.void_reason}</p>}
      </div>
    );
  }
  if (e.extension) {
    return (
      <p className="mt-1 text-sm">
        {e.extension.package_name} · hạn {formatDate(e.extension.previous_expires_on)} → {formatDate(e.extension.expires_on)}
        <span className="text-text-muted"> · {e.extension.reason} · {e.extension.by_name}</span>
      </p>
    );
  }
  if (e.cancellation) {

    return (
      <p className="mt-1 text-sm">
        {e.cancellation.package_name} · {e.cancellation.reason}
        {e.cancellation.refund && <span className="text-text-muted"> · hoàn {money(e.cancellation.refund.amount)}</span>}
      </p>
    );
  }
  return e.void ? <p className="mt-1 text-sm text-text-muted">{e.void.reason}</p> : null;
}

/** Search, kind and dates over the history (EM round 3 item 6), kept in the profile's URL. */
function HistoryFilters() {
  const [s, set] = useSearchState(historySearch);
  const today = spaToday();
  const active = [s.hkind, s.hfrom, s.hto].filter(Boolean).length;
  const reset = () => {
    set({ hq: "", hkind: "", hfrom: "", hto: "" });
  };
  return (
    <ResponsiveFilters
      label="Lọc lịch sử"
      activeCount={active}
      onReset={reset}
      search={
        <SearchField
          label="Tìm dịch vụ, gói hoặc kỹ thuật viên"
          placeholder="Dịch vụ, KTV…"
          className="history-search"
          value={s.hq}
          onCommit={(hq) => {
            set({ hq });
          }}
        />
      }
    >
      {/* The profile's column is narrow (~420px at 1280): search + kind on one row, the date range
          on the next as two equal fields (EM polish round 2: one field per row before). */}
      <FilterSelect
        label="Loại"
        value={s.hkind}
        options={KIND_OPTIONS}
        onChange={(hkind) => {
          set({ hkind });
        }}
      />
      <div className="history-filters">
        <DateField
          label="Từ ngày"
          value={s.hfrom}
          max={s.hto || today}
          onChange={(hfrom) => {
            set({ hfrom });
          }}
        />
        <DateField
          label="Đến ngày"
          value={s.hto}
          min={s.hfrom || undefined}
          max={today}
          onChange={(hto) => {
            set({ hto });
          }}
        />
      </div>
      {(s.hq || active > 0) && (
        <button type="button" className="btn btn-secondary max-sm:hidden" onClick={reset}>
          Xóa bộ lọc
        </button>
      )}
    </ResponsiveFilters>
  );
}

/** The history request for the URL's filters (also what the route loader prefetches). */
export function historyFilterOf(s: ReturnType<typeof historySearch.parse>): HistoryFilter {
  // A reversed range is not sent (the API would answer 422); the inputs' min/max make it rare.
  const range = s.hfrom && s.hto && s.hfrom > s.hto ? {} : { from: s.hfrom || undefined, to: s.hto || undefined };
  const q = s.hq.trim();
  return { q: q.length >= 2 ? q : undefined, kind: s.hkind || undefined, ...range };
}

/**
 * Every query of the profile at once, from the id in the URL (perf page §2d: the panels used to
 * wait for `/customers/{id}` before they mounted, one round trip more). Never throws: each panel
 * shows its own error. Same gates as the panels, so no request is sent only to be refused.
 */
export function prefetchCustomerPage(queryClient: QueryClient, customerId: string, rawSearch: Record<string, unknown>): void {
  const auth = queryClient.getQueryData(authQuery.queryKey);
  const me = auth?.state === "signed_in" ? auth.me : null;
  void queryClient.query(customerQuery(customerId)).catch(() => undefined);
  void queryClient.query(customerPackagesQuery(customerId)).catch(() => undefined);
  if (!canSeeManagement(me)) return;
  void queryClient.query(followUpsQuery(customerId)).catch(() => undefined);
  void queryClient.query(customerStatsQuery(customerId)).catch(() => undefined);
  void queryClient.infiniteQuery(customerHistoryQuery(customerId, historyFilterOf(historySearch.parse(rawSearch)))).catch(() => undefined);
}

function Timeline({ customerId, permissions }: { customerId: string; permissions: readonly string[] }) {
  const [s] = useSearchState(historySearch);
  const today = spaToday();
  const filtered = Boolean(s.hq || s.hkind || s.hfrom || s.hto);
  const history = useInfiniteQuery(customerHistoryQuery(customerId, historyFilterOf(s)));
  if (history.isPending) return <RowsSkeleton rows={4} />;
  if (history.isError) return <ErrorState error={history.error} onRetry={() => void history.refetch()} />;
  const events = history.data.pages.flatMap((p) => p.events);
  if (events.length === 0)
    return filtered ? (
      <EmptyState title="Không thấy mục nào">Thử bỏ bớt bộ lọc hoặc tìm bằng tên khác.</EmptyState>
    ) : (
      <EmptyState title="Chưa có lịch sử">Lượt khách và gói đã mua sẽ hiện ở đây.</EmptyState>
    );
  const monthOf = (e: HistoryEvent) => monthLabel(e.business_date);
  return (
    <div className={`grid content-start gap-3 ${history.isPlaceholderData ? "is-refreshing" : ""}`}>
      <ol className="timeline" aria-label="Lịch sử sử dụng">
        {events.map((e, i) => {
          const m = monthOf(e);
          const prev = events[i - 1];
          const heading = !prev || monthOf(prev) !== m ? m : null;
          const [icon, dot] = DOT[e.kind];
          // One chip per event (design-system.md "Quy tắc" 7); a voided entry reads muted.
          const { title, chip } = eventHead(e);
          const voided = e.kind.endsWith("_voided");
          const correctable =
            e.kind === "visit" &&
            !!e.visit &&
            mayCorrectVisit(permissions, { business_date: e.business_date, voided: e.visit.voided }, today);
          return (
            <li key={`${e.at}-${String(i)}`} className="relative">
              {heading && <p className="timeline-month">{heading}</p>}
              <div className={`relative rounded-md border border-border p-3 ${voided ? "bg-surface-2 text-text-muted" : "bg-surface"}`}>
                <span aria-hidden="true" className={`timeline-dot ${dot} text-on-primary`}>
                  <Icon name={icon} className="size-2.5" />
                </span>
                {/* Title + chip left; when and "Sửa lượt" right, as the "Sửa" of the price list
                    (EM polish round 2: it was a bare link under the lines). */}
                <div className="flex flex-wrap items-center justify-between gap-x-3 gap-y-1">
                  <span className="flex flex-wrap items-center gap-2">
                    <span className="font-semibold">{title}</span>
                    {chip && <span className={`badge ${chip[1]}`}>{chip[0]}</span>}
                  </span>
                  <span className="ml-auto flex flex-wrap items-center justify-end gap-x-3 gap-y-1">
                    <span className="text-xs text-text-muted tabular">
                      {relativeDay(e.business_date)} · {time.format(new Date(e.at))}
                    </span>
                    {e.visit && correctable && (
                      <Link to="/quay" search={{ sua: e.visit.id }} className="btn btn-secondary">
                        Sửa lượt
                      </Link>
                    )}
                  </span>
                </div>
                <EventBody e={e} />
              </div>
            </li>
          );
        })}
      </ol>
      {history.hasNextPage && (
        <button
          type="button"
          className="btn btn-secondary justify-self-center"
          aria-busy={history.isFetchingNextPage || undefined}
          onClick={() => {
            if (!history.isFetchingNextPage) void history.fetchNextPage();
          }}
        >
          Xem thêm lịch sử
        </button>
      )}
    </div>
  );
}

/** "Chăm sóc lại": every contact with this customer (newest first) and "Ghi lần liên hệ". */
function FollowUps({ customerId, customerName, canRecord }: { customerId: string; customerName: string; canRecord: boolean }) {
  const list = useQuery(followUpsQuery(customerId));
  const [open, setOpen] = useState(false);
  const today = spaToday();
  const next = list.data?.items[0]?.next_follow_up_on;
  return (
    <section aria-labelledby="follow-title" className="panel">
      <div className="panel-head">
        <span className="flex min-w-0 flex-wrap items-center gap-2">
          <h2 id="follow-title" className="panel-title">
            Chăm sóc lại
          </h2>
          {next && (
            <span className={`badge ${next <= today ? "badge-warn" : "badge-info"}`}>
              {next <= today ? "Đến hạn liên hệ" : "Hẹn liên hệ lại"} {relativeDay(next)}
            </span>
          )}
        </span>
        {canRecord && (
          <button
            type="button"
            className="btn btn-secondary"
            onClick={() => {
              setOpen(true);
            }}
          >
            <Icon name="plus" className="size-4" /> Ghi lần liên hệ
          </button>
        )}
      </div>
      <BottomSheet
        open={open}
        onClose={() => {
          setOpen(false);
        }}
        title="Ghi lần liên hệ"
        description={customerName}
      >
        <FollowUpForm
          customerId={customerId}
          customerName={customerName}
          onDone={() => {
            setOpen(false);
          }}
          onCancel={() => {
            setOpen(false);
          }}
        />
      </BottomSheet>
      {list.isPending && <RowsSkeleton rows={2} />}
      {list.isError && <ErrorState error={list.error} onRetry={() => void list.refetch()} />}
      {list.data && list.data.items.length === 0 && (
        <p className="empty-note">Chưa liên hệ lần nào.{canRecord ? " Gọi hoặc nhắn khách xong, bấm “Ghi lần liên hệ” để hẹn lần sau." : ""}</p>
      )}
      {list.data && list.data.items.length > 0 && (
        <ol className="row-list" aria-label="Các lần liên hệ">
          {list.data.items.map((f) => (
            <li key={f.id}>
              <div className="row-head">
                <span className="row-title">{OUTCOME_LABEL[f.outcome]}</span>
                <span className="row-meta tabular">{relativeDay(f.contacted_on)}</span>
              </div>
              <p className="row-meta">
                {CHANNEL_LABEL[f.channel]} · {f.by_name}
                {f.next_follow_up_on && ` · hẹn lại ${relativeDay(f.next_follow_up_on)}`}
              </p>
              {f.note && <p>{f.note}</p>}
            </li>
          ))}
        </ol>
      )}
      {list.data?.has_more && <p className="row-meta">Chỉ hiện 100 lần gần nhất.</p>}
    </section>
  );
}

/**
 * Customer profile, management side: the follow-up log, then the usage history (summary numbers,
 * a searchable timeline of visits, purchases, voids, corrections and cancellations, newest
 * first). Shown only where the management views exist (app/features.ts); the API decides each part.
 */
export function CustomerHistory({ customerId, customerName = "", me }: { customerId: string; customerName?: string; me: Me | null }) {
  if (!canSeeManagement(me)) return null;
  return (
    <div className="khach-stack">
      <FollowUps customerId={customerId} customerName={customerName} canRecord={can(me, "customer.follow_up")} />
      <section aria-labelledby="history-title" className="panel">
        <h2 id="history-title" className="panel-title">
          Lịch sử sử dụng
        </h2>
        <Summary customerId={customerId} />
        <HistoryFilters />
        <Timeline customerId={customerId} permissions={me?.permissions ?? []} />
      </section>
    </div>
  );
}
