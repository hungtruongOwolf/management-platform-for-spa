import { useQuery } from "@tanstack/react-query";
import { PageHeader } from "../../app/PageHeader";
import { useSearchState } from "../../lib/searchState";
import { spaToday } from "../corrections/api";
import { timeOf } from "../front-desk/api";
import { EmptyState, ErrorState, RowsSkeleton } from "../reports/ui";
import { myWorkQuery, type ServedVisit, type UpcomingAppointment } from "./api";
import { monthLabel } from "./labels";
import { myWorkSearch } from "./search";
import { MonthPicker, dayLabel } from "./ui";
import "./staff.css";

/** Items grouped by business day, keeping the order they arrived in. */
function byDay<T extends { business_date: string }>(items: T[]): [string, T[]][] {
  const groups = new Map<string, T[]>();
  for (const it of items) groups.set(it.business_date, [...(groups.get(it.business_date) ?? []), it]);
  return [...groups.entries()];
}

/**
 * "Khách của tôi" (/khach-cua-toi, spec staff-management §10 US9): a technician's upcoming
 * appointments and the customers they served this month. The server picks the person from the
 * session and sends only names, times and services — nothing here can ask for someone else.
 */
export function MyWorkPage() {
  const [search, patch] = useSearchState(myWorkSearch);
  const today = spaToday();
  const current = today.slice(0, 7);
  const month = search.month || current;
  const query = useQuery(myWorkQuery(month));

  return (
    <section className="grid content-start gap-5">
      <PageHeader title="Khách của tôi">
        <MonthPicker
          month={month}
          current={current}
          onChange={(m) => {
            patch({ month: m === current ? "" : m });
          }}
        />
      </PageHeader>
      {query.isPending ? (
        <RowsSkeleton rows={5} />
      ) : query.isError ? (
        <ErrorState error={query.error} onRetry={() => void query.refetch()} />
      ) : !query.data.linked || !query.data.data ? (
        <EmptyState title="Tài khoản chưa gắn với hồ sơ nhân viên" icon="id-card">
          Nhờ chủ spa gắn tài khoản này vào hồ sơ của bạn để xem lịch hẹn và khách đã làm.
        </EmptyState>
      ) : (
        <>
          <Upcoming items={query.data.data.upcoming} today={query.data.data.today} />
          <Served items={query.data.data.served} month={query.data.data.month} />
        </>
      )}
    </section>
  );
}

function Upcoming({ items, today }: { items: UpcomingAppointment[]; today: string }) {
  return (
    <section className="card grid gap-3 p-4 sm:p-5" aria-labelledby="my-upcoming">
      <h2 id="my-upcoming" className="section-title">
        Lịch hẹn sắp tới
      </h2>
      {items.length === 0 ? (
        <p className="text-text-muted">Chưa có ai đặt lịch với bạn trong 30 ngày tới. Lịch quầy đặt cho bạn sẽ hiện ở đây.</p>
      ) : (
        byDay(items).map(([day, list]) => (
          <div key={day} className="grid gap-2">
            <h3 className="text-sm font-semibold text-text-muted">{day === today ? `Hôm nay · ${dayLabel(day)}` : dayLabel(day)}</h3>
            <ul className="work-list">
              {list.map((a) => (
                <li key={a.appointment_id} className="work-row">
                  <span className="w-28 shrink-0 font-semibold tabular">
                    {timeOf(a.starts_at)}–{timeOf(a.ends_at)}
                  </span>
                  <div className="min-w-0 flex-1">
                    <p className="font-semibold">{a.customer_name}</p>
                    <p className="text-sm text-text-muted">{a.services.join(", ")}</p>
                  </div>
                  <span className={`badge ${a.status === "arrived" ? "badge-ok" : "badge-info"}`}>{a.status === "arrived" ? "Khách đã đến" : "Đã đặt"}</span>
                </li>
              ))}
            </ul>
          </div>
        ))
      )}
    </section>
  );
}

function Served({ items, month }: { items: ServedVisit[]; month: string }) {
  const sessions = items.reduce((n, v) => n + v.services.reduce((m, s) => m + s.qty, 0), 0);
  return (
    <section className="card grid gap-3 p-4 sm:p-5" aria-labelledby="my-served">
      <h2 id="my-served" className="section-title">
        Khách đã làm · {monthLabel(month)}
      </h2>
      {items.length === 0 ? (
        <p className="text-text-muted">Chưa có lượt khách nào trong tháng này. Chọn tháng trước ở góc trên để xem lại.</p>
      ) : (
        <>
          <p className="text-sm">
            <strong>{items.length}</strong> lượt khách · <strong>{sessions}</strong> lần làm dịch vụ
          </p>
          {byDay(items).map(([day, list]) => (
            <div key={day} className="grid gap-2">
              <h3 className="text-sm font-semibold text-text-muted">{dayLabel(day)}</h3>
              <ul className="work-list">
                {list.map((v) => (
                  <li key={v.visit_id} className="work-row">
                    <span className="w-14 shrink-0 font-semibold tabular">{timeOf(v.check_in_at)}</span>
                    <div className="min-w-0 flex-1">
                      <p className="font-semibold">{v.customer_name}</p>
                      <p className="text-sm text-text-muted">
                        {v.services
                          .map((s) => `${s.service_name}${s.qty > 1 ? ` ×${String(s.qty)}` : ""}${s.from_package ? " (gói)" : ""}`)
                          .join(", ")}
                      </p>
                    </div>
                  </li>
                ))}
              </ul>
            </div>
          ))}
        </>
      )}
    </section>
  );
}
