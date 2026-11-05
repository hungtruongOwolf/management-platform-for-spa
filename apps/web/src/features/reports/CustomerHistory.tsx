import type { HistoryKind } from "@vpyd/api-client";
import { useInfiniteQuery, useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { canSeeManagement } from "../../app/features";
import { Icon, type IconName } from "../../app/Icon";
import { monthLabel } from "../../lib/format";
import { serviceLabel } from "../../lib/names";
import { useSearchState } from "../../lib/searchState";
import { can, type Me } from "../auth/session";
import { spaToday } from "../corrections/api";
import { CHANNEL_LABEL, OUTCOME_LABEL, followUpsQuery } from "../customers/followUps";
import { FollowUpForm } from "../customers/FollowUpForm";
import { HISTORY_KINDS, historySearch } from "../customers/search";
import { customerPackagesQuery } from "../sales/api";
import { customerHistoryQuery, customerStatsQuery, relativeDay } from "./api";
import type { HistoryEvent } from "./contract";
import { BottomSheet } from "../../components/BottomSheet";
import { DateField } from "../../components/DateField";
import { FilterSelect, ResponsiveFilters, SearchField } from "./filters";
import { EmptyState, ErrorState, RowsSkeleton, money } from "./ui";

const time = new Intl.DateTimeFormat("vi-VN", { hour: "2-digit", minute: "2-digit", timeZone: "Asia/Ho_Chi_Minh" });

const KIND: Record<HistoryKind, [string, IconName, string]> = {
  visit: ["Lượt khách", "desk", "bg-primary"],
  purchase: ["Mua gói liệu trình", "package", "bg-accent"],
  visit_voided: ["Lượt đã hủy", "x", "bg-border-strong"],
  purchase_voided: ["Hóa đơn đã hủy", "x", "bg-border-strong"],
  package_cancelled: ["Hủy gói", "alert", "bg-danger"],
};

const KIND_OPTIONS: readonly (readonly [HistoryKind | "", string])[] = [["", "Mọi loại"], ...HISTORY_KINDS.map((k) => [k, KIND[k][0]] as const)];

function Fact({ label, value, hint }: { label: string; value: string; hint?: string | undefined }) {
  return (
    <div className="rounded-[10px] bg-surface-2 p-3">
      <dt className="text-xs text-text-muted">{label}</dt>
      <dd className="mt-0.5 text-lg font-semibold tabular">{value}</dd>
      {hint && <dd className="text-xs text-text-muted">{hint}</dd>}
    </div>
  );
}

/**
 * "Đã chi" counts money taken through the app only. A package carried over from the paper books
 * (no sale) was paid before the app, so the figure would read as "0đ" for a long-time customer
 * (EM round 4): say what it covers instead of a bare zero.
 */
function spentHint(refunded: number, carriedOver: boolean): string | undefined {
  const parts = [refunded ? `đã hoàn ${money(refunded)}` : "", carriedOver ? "chưa gồm tiền gói trả trước khi dùng app" : ""].filter(Boolean);
  return parts.length > 0 ? parts.join(" · ") : undefined;
}

function Summary({ customerId }: { customerId: string }) {
  const stats = useQuery(customerStatsQuery(customerId));
  // Same query (and cache entry) as the packages card: no extra request.
  const packages = useQuery(customerPackagesQuery(customerId));
  const carriedOver = packages.data?.some((p) => !p.sale_id) ?? false;
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
      <Fact label="Đã chi qua app" value={money(s.net_spent)} hint={spentHint(s.refunded, carriedOver)} />
    </dl>
  );
}

function EventBody({ e }: { e: HistoryEvent }) {
  if (e.visit) {
    return (
      <ul className="mt-1 grid content-start gap-1 text-sm">
        {e.visit.lines.map((l, i) => (
          <li key={i} className="flex flex-wrap justify-between gap-x-3">
            <span>
              {serviceLabel(l.service_name, l.category_name)}
              <span className="text-text-muted">
                {l.source === "package" ? ` · trừ buổi${l.sessions_left_after != null ? `, còn ${String(l.sessions_left_after)}` : ""}` : " · dịch vụ lẻ"} · KTV {l.staff_name}
              </span>
            </span>
            {l.amount > 0 && <span className="tabular">{money(l.amount)}</span>}
          </li>
        ))}
        {e.visit.void_reason && <li className="text-text-muted">Lý do hủy: {e.visit.void_reason}</li>}
      </ul>
    );
  }
  if (e.purchase) {
    return (
      <div className="mt-1 text-sm">
        {e.purchase.lines.map((l, i) => (
          <p key={i}>{l.description}</p>
        ))}
        <p className="text-text-muted">
          Thanh toán <span className="font-semibold text-text tabular">{money(e.purchase.total)}</span>
          {e.purchase.discount_amount > 0 && ` (giảm ${money(e.purchase.discount_amount)})`}
          {e.purchase.closer_name && ` · thu bởi ${e.purchase.closer_name}`}
        </p>
        {e.purchase.void_reason && <p className="text-text-muted">Lý do hủy: {e.purchase.void_reason}</p>}
      </div>
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
          className="basis-full"
          value={s.hq}
          onCommit={(hq) => {
            set({ hq });
          }}
        />
      }
    >
      <FilterSelect
        label="Loại"
        value={s.hkind}
        options={KIND_OPTIONS}
        onChange={(hkind) => {
          set({ hkind });
        }}
      />
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
      {(s.hq || active > 0) && (
        <button type="button" className="btn btn-secondary max-sm:hidden" onClick={reset}>
          Xóa bộ lọc
        </button>
      )}
    </ResponsiveFilters>
  );
}

function Timeline({ customerId }: { customerId: string }) {
  const [s] = useSearchState(historySearch);
  const filtered = Boolean(s.hq || s.hkind || s.hfrom || s.hto);
  // A reversed range is not sent (the API would answer 422); the inputs' min/max make it rare.
  const range = s.hfrom && s.hto && s.hfrom > s.hto ? {} : { from: s.hfrom || undefined, to: s.hto || undefined };
  const q = s.hq.trim();
  const history = useInfiniteQuery(customerHistoryQuery(customerId, { q: q.length >= 2 ? q : undefined, kind: s.hkind || undefined, ...range }));
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
          const [label, icon, dot] = KIND[e.kind];
          const voided = e.kind.endsWith("_voided");
          return (
            <li key={`${e.at}-${String(i)}`} className="relative">
              {heading && <p className="timeline-month">{heading}</p>}
              <div className={`relative rounded-[10px] border border-border p-3 ${voided ? "bg-surface-2 text-text-muted" : "bg-surface"}`}>
                <span aria-hidden="true" className={`timeline-dot ${dot} text-on-primary`}>
                  <Icon name={icon} className="size-2.5" />
                </span>
                <p className="flex flex-wrap items-baseline justify-between gap-2">
                  <span className={`font-semibold ${voided ? "line-through" : ""}`}>{label}</span>
                  <span className="text-xs text-text-muted tabular">
                    {relativeDay(e.business_date)} · {time.format(new Date(e.at))}
                  </span>
                </p>
                <EventBody e={e} />
              </div>
            </li>
          );
        })}
      </ol>
      {history.hasNextPage && (
        <button type="button" className="btn btn-secondary justify-self-center" disabled={history.isFetchingNextPage} onClick={() => void history.fetchNextPage()}>
          {history.isFetchingNextPage ? "Đang tải…" : "Xem thêm lịch sử"}
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
    <section aria-labelledby="follow-title" className="card grid content-start gap-3 p-4">
      <div className="card-head">
        <h2 id="follow-title" className="card-title">
          <Icon name="phone" className="size-5 text-accent" /> Chăm sóc lại
        </h2>
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
      {next && (
        <p className={`badge justify-self-start ${next <= today ? "badge-danger" : "badge-ok"}`}>
          {next <= today ? "Đến hạn liên hệ" : "Hẹn liên hệ lại"} {relativeDay(next)}
        </p>
      )}
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
      {list.data && list.data.items.length === 0 && <p className="text-sm text-text-muted">Chưa liên hệ lần nào.</p>}
      {list.data && list.data.items.length > 0 && (
        <ol className="timeline" aria-label="Các lần liên hệ">
          {list.data.items.map((f) => (
            <li key={f.id} className="relative">
              <div className="relative rounded-[10px] border border-border bg-surface p-3">
                <span aria-hidden="true" className="timeline-dot bg-accent text-on-primary">
                  <Icon name="phone" className="size-2.5" />
                </span>
                <p className="flex flex-wrap items-baseline justify-between gap-2">
                  <span className="font-semibold">{OUTCOME_LABEL[f.outcome]}</span>
                  <span className="text-xs text-text-muted tabular">{relativeDay(f.contacted_on)}</span>
                </p>
                <p className="text-sm text-text-muted">
                  {CHANNEL_LABEL[f.channel]} · {f.by_name}
                  {f.next_follow_up_on && ` · hẹn lại ${relativeDay(f.next_follow_up_on)}`}
                </p>
                {f.note && <p className="mt-1 text-sm">{f.note}</p>}
              </div>
            </li>
          ))}
        </ol>
      )}
      {list.data?.has_more && <p className="text-xs text-text-muted">Chỉ hiện 100 lần gần nhất.</p>}
    </section>
  );
}

/**
 * Customer profile, management side: the follow-up log, then the usage history (summary numbers,
 * a searchable timeline of visits, purchases, voids and cancellations, newest first). Shown only
 * where the management views exist (app/features.ts); the API decides each part.
 */
export function CustomerHistory({ customerId, customerName = "", me }: { customerId: string; customerName?: string; me: Me | null }) {
  if (!canSeeManagement(me)) return null;
  return (
    <div className="grid content-start gap-4">
      <FollowUps customerId={customerId} customerName={customerName} canRecord={can(me, "customer.follow_up")} />
      <section aria-labelledby="history-title" className="card grid content-start gap-4 p-4">
        <h2 id="history-title" className="card-title">
          <Icon name="history" className="size-5 text-accent" /> Lịch sử sử dụng
        </h2>
        <Summary customerId={customerId} />
        <HistoryFilters />
        <Timeline customerId={customerId} />
      </section>
    </div>
  );
}
