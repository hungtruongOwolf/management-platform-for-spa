import { useQuery } from "@tanstack/react-query";
import { canSeeManagement } from "../../app/features";
import { Icon, type IconName } from "../../app/Icon";
import type { Me } from "../auth/session";
import { customerHistoryQuery, customerStatsQuery, relativeDay } from "./api";
import type { HistoryEvent } from "./contract";
import { EmptyState, ErrorState, RowsSkeleton, money } from "./ui";

const time = new Intl.DateTimeFormat("vi-VN", { hour: "2-digit", minute: "2-digit", timeZone: "Asia/Ho_Chi_Minh" });
const month = new Intl.DateTimeFormat("vi-VN", { month: "long", year: "numeric", timeZone: "UTC" });

const KIND: Record<HistoryEvent["kind"], [string, IconName, string]> = {
  visit: ["Lượt khách", "desk", "bg-primary"],
  purchase: ["Mua gói liệu trình", "package", "bg-accent"],
  visit_voided: ["Lượt đã hủy", "x", "bg-border-strong"],
  purchase_voided: ["Hóa đơn đã hủy", "x", "bg-border-strong"],
  package_cancelled: ["Hủy gói", "alert", "bg-danger"],
};

function Fact({ label, value, hint }: { label: string; value: string; hint?: string | undefined }) {
  return (
    <div className="rounded-[10px] bg-surface-2 p-3">
      <dt className="text-xs text-text-muted">{label}</dt>
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
      <Fact label="Đã chi" value={money(s.net_spent)} hint={s.refunded ? `đã hoàn ${money(s.refunded)}` : undefined} />
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
              {l.service_name}
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

function Timeline({ customerId }: { customerId: string }) {
  const history = useQuery(customerHistoryQuery(customerId));
  if (history.isPending) return <RowsSkeleton rows={4} />;
  if (history.isError) return <ErrorState error={history.error} onRetry={() => void history.refetch()} />;
  if (history.data.events.length === 0)
    return <EmptyState title="Chưa có lịch sử">Lượt khách và gói đã mua sẽ hiện ở đây.</EmptyState>;
  const monthOf = (e: HistoryEvent) => month.format(new Date(`${e.business_date}T00:00:00Z`));
  const events = history.data.events;
  return (
    <ol className="timeline" aria-label="Lịch sử sử dụng">
      {events.map((e, i) => {
        const m = monthOf(e);
        const prev = events[i - 1];
        const heading = !prev || monthOf(prev) !== m ? m : null;
        const [label, icon, dot] = KIND[e.kind];
        const voided = e.kind.endsWith("_voided");
        return (
          <li key={`${e.at}-${String(i)}`} className="relative">
            {heading && <p className="mt-2 mb-1 eyebrow first-letter:uppercase">{heading}</p>}
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
  );
}

/**
 * Customer profile: summary numbers and the usage timeline (visits, purchases, voids,
 * cancellations), newest first. Shown only where the management views exist (app/features.ts).
 */
export function CustomerHistory({ customerId, me }: { customerId: string; me: Me | null }) {
  if (!canSeeManagement(me)) return null;
  return (
    <section aria-labelledby="history-title" className="card grid content-start gap-4 p-4">
      <h2 id="history-title" className="flex items-center gap-2 text-lg font-semibold">
        <Icon name="history" className="size-5 text-accent" /> Lịch sử sử dụng
      </h2>
      <Summary customerId={customerId} />
      <Timeline customerId={customerId} />
    </section>
  );
}
