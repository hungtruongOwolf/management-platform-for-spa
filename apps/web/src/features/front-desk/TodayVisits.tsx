// Today's visits on /quay (full width, EM round 4 item 6): rows open inline with the lines,
// payment, phone, health warning and a link to the profile. A lazy chunk: it renders after the
// first paint anyway (its query waits for it), so it stays off the startup bundle.
import type { Visit } from "@vpyd/api-client";
import {
  keepPreviousData,
  useQuery,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { formatVnd } from "@vpyd/ui";
import { Suspense, lazy, useState } from "react";
import { CustomerName } from "../../app/CustomerName";
import { DateField } from "../../components/DateField";
import { useAfterFirstPaint } from "../../lib/afterFirstPaint";
import { messageFor } from "../../lib/errors";
import { invalidateManagementViews } from "../../lib/invalidate";
import { serviceLabel } from "../../lib/names";
import { can, meQuery } from "../auth/session";
import { spaToday } from "../corrections/api";
import { customerQuery } from "../customers/api";
import { HealthNote } from "../customers/HealthNote";
import {
  VISITS_PAGE,
  paymentText,
  timeOf,
  todayAllVisitsQuery,
  todayVisitsQuery,
  visitsOfDayQuery,
} from "./api";

const VoidVisitForm = lazy(() =>
  import("../corrections/VoidVisitForm").then((m) => ({
    default: m.VoidVisitForm,
  })),
);

const card = "rounded-[14px] border border-border bg-surface p-4 shadow-card";

/** Inline details of one visit: lines, technicians, payment, the customer's phone + a profile link. */
function VisitDetails({ v }: { v: Visit }) {
  const customer = useQuery(customerQuery(v.customer_id));
  return (
    <div className="mt-2 grid gap-2 rounded-[10px] bg-surface-2 p-3 text-sm">
      <ul className="grid gap-1">
        {v.lines.map((l) => (
          <li key={l.id} className="flex flex-wrap justify-between gap-2">
            <span>
              {serviceLabel(l.service_name, l.category_name)} ·{" "}
              {l.source === "package"
                ? `trừ buổi ${l.package_name ?? ""}${l.sessions_left_after != null ? ` (còn ${String(l.sessions_left_after)}/${String(l.sessions_total ?? "?")})` : ""}`
                : "dịch vụ lẻ"}{" "}
              · KTV {l.staff_name}
            </span>
            <span className="tabular-nums">
              {l.source === "one_off" ? formatVnd(l.line_value) : "—"}
            </span>
          </li>
        ))}
      </ul>
      {v.discount_amount > 0 && (
        <p>
          Giảm {formatVnd(v.discount_amount)}
          {v.discount_reason ? ` · ${v.discount_reason}` : ""}
        </p>
      )}
      <p>{paymentText(v, formatVnd)}</p>
      {v.business_date < v.created_at.slice(0, 10) && (
        <p className="text-text-muted">
          Nhập bù lúc {new Date(v.created_at).toLocaleString("vi-VN")}
        </p>
      )}
      <p className="flex flex-wrap items-center gap-x-3 gap-y-1">
        {customer.data && (
          <span className="tabular-nums">SĐT {customer.data.phone}</span>
        )}
        <Link
          to="/khach-hang/$customerId"
          params={{ customerId: v.customer_id }}
          className="font-semibold text-primary underline"
        >
          Mở hồ sơ khách
        </Link>
      </p>
      {customer.data && <HealthNote customer={customer.data} />}
    </div>
  );
}

export function TodayVisits() {
  const { data: me } = useSuspenseQuery(meQuery);
  const queryClient = useQueryClient();
  const todayIso = spaToday();
  const [day, setDay] = useState(todayIso);
  const [voiding, setVoiding] = useState<string | null>(null);
  const [expanded, setExpanded] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  // A busy day holds a hundred visits: the newest VISITS_PAGE first; "Xem tất cả" loads the day,
  // then shows it VISITS_PAGE at a time.
  const [all, setAll] = useState(false);
  const [shown, setShown] = useState(VISITS_PAGE);
  const isToday = day === todayIso;
  const painted = useAfterFirstPaint();
  const today = useQuery({
    ...(isToday
      ? all
        ? todayAllVisitsQuery
        : todayVisitsQuery
      : visitsOfDayQuery(day, all)),
    enabled: painted,
    placeholderData: keepPreviousData,
  });
  const list = today.data ?? [];
  const hasMore = !all && list.length > VISITS_PAGE;
  const visible = list.slice(0, all ? shown : VISITS_PAGE);
  // UX only: the server decides (403 day.locked for managers; owners may).
  const mayVoid =
    can(me, "record.void_same_day") &&
    (isToday || can(me, "record.void_closed_day"));
  return (
    <section aria-labelledby="today-title" className={card}>
      <div className="mb-2 flex flex-wrap items-end justify-between gap-2">
        <h2 id="today-title" className="font-semibold">
          {isToday
            ? "Lượt khách hôm nay"
            : `Lượt khách ngày ${day.split("-").reverse().join("/")}`}
          {today.data
            ? ` (${hasMore ? `${String(VISITS_PAGE)}+` : String(list.length)})`
            : ""}
        </h2>
        <DateField
          label="Xem ngày"
          value={day}
          max={todayIso}
          onChange={(iso) => {
            setDay(iso || todayIso);
            setVoiding(null);
            setExpanded(null);
            setAll(false);
            setShown(VISITS_PAGE);
          }}
          className="flex items-center gap-2 text-sm"
        />
      </div>
      {!isToday && can(me, "record.void_same_day") && !mayVoid && (
        <p className="mb-2 text-sm text-text-muted">
          Ngày đã chốt: chỉ chủ spa hủy được lượt của ngày này.
        </p>
      )}
      {notice && (
        <p
          role="status"
          className="mb-2 rounded-[10px] bg-primary-soft p-2 text-sm"
        >
          {notice}
        </p>
      )}
      {today.isPending && (
        <div aria-hidden="true" className="grid content-start gap-2">
          <div className="h-10 skeleton" />
          <div className="h-10 skeleton" />
        </div>
      )}
      {today.isError && (
        <p role="alert" className="text-danger">
          {messageFor(today.error)}{" "}
          <button
            type="button"
            className="underline"
            onClick={() => void today.refetch()}
          >
            Thử lại
          </button>
        </p>
      )}
      {today.data?.length === 0 && (
        <p className="text-text-muted">
          Chưa có khách nào hôm nay. Lượt vừa lưu sẽ hiện ở đây.
        </p>
      )}
      {today.data && today.data.length > 0 && (
        <ul
          aria-label={isToday ? "Lượt khách hôm nay" : "Lượt khách trong ngày"}
        >
          {visible.map((v) => {
            const open = expanded === v.id;
            return (
              <li
                key={v.id}
                className="border-b border-dashed border-border py-2 text-sm last:border-b-0"
              >
                <div className="flex items-start justify-between gap-2">
                  <button
                    type="button"
                    aria-expanded={open}
                    onClick={() => {
                      setExpanded(open ? null : v.id);
                    }}
                    className={`grid min-w-0 flex-1 gap-x-4 gap-y-0.5 rounded-[6px] text-left hover:bg-surface-2 md:grid-cols-[56px_minmax(0,1.2fr)_minmax(0,2fr)_minmax(0,1.3fr)] ${v.status === "voided" ? "text-text-muted" : ""}`}
                  >
                    <strong className="tabular-nums">
                      {timeOf(v.check_in_at)}
                    </strong>
                    <span className="min-w-0">
                      <CustomerName
                        name={v.customer_name}
                        gender={v.customer_gender}
                      />
                      {v.status === "voided" && (
                        <span className="badge badge-danger ml-2 align-middle">
                          Đã hủy
                        </span>
                      )}
                    </span>
                    <span className="text-text-muted">
                      {v.lines
                        .map(
                          (l) =>
                            `${serviceLabel(l.service_name, l.category_name)}${l.source === "package" ? " (trừ buổi)" : ""}`,
                        )
                        .join(", ")}{" "}
                      · KTV{" "}
                      {[...new Set(v.lines.map((l) => l.staff_name))].join(
                        ", ",
                      )}
                    </span>
                    <span className="text-text-muted">
                      {paymentText(v, formatVnd)}
                    </span>
                    {v.status === "voided" && v.void_reason && (
                      <span className="md:col-span-4">
                        Lý do hủy: {v.void_reason}
                      </span>
                    )}
                  </button>
                  <span className="flex shrink-0 items-start gap-2">
                    <span
                      className={`tabular-nums ${v.status === "voided" ? "text-text-muted line-through" : ""}`}
                    >
                      {v.total ? formatVnd(v.total) : "—"}
                    </span>
                    {mayVoid &&
                      v.status === "completed" &&
                      voiding !== v.id && (
                        <button
                          type="button"
                          aria-label={`Hủy lượt ${timeOf(v.check_in_at)} của ${v.customer_name}`}
                          onClick={() => {
                            setNotice(null);
                            setVoiding(v.id);
                          }}
                          className="min-h-11 rounded-[10px] border border-border px-3 font-medium text-danger"
                        >
                          Hủy
                        </button>
                      )}
                  </span>
                </div>
                {open && <VisitDetails v={v} />}
                {voiding === v.id && (
                  <Suspense
                    fallback={
                      <p role="status" className="w-full text-text-muted">
                        Đang mở…
                      </p>
                    }
                  >
                    <div className="w-full">
                      <VoidVisitForm
                        visit={v}
                        onCancel={() => {
                          setVoiding(null);
                        }}
                        onDone={(voided) => {
                          setVoiding(null);
                          setNotice(`Đã hủy lượt của ${voided.customer_name}.`);
                          void queryClient.invalidateQueries({
                            queryKey: ["visits"],
                          });
                          void queryClient.invalidateQueries({
                            queryKey: ["customer-packages"],
                          });
                          void queryClient.invalidateQueries({
                            queryKey: ["appointments"],
                          });
                          void invalidateManagementViews(
                            queryClient,
                            voided.customer_id,
                          );
                        }}
                      />
                    </div>
                  </Suspense>
                )}
              </li>
            );
          })}
        </ul>
      )}
      {hasMore && (
        <button
          type="button"
          onClick={() => {
            setAll(true);
            setShown(VISITS_PAGE * 2);
          }}
          className="mt-2 min-h-11 w-full rounded-[10px] border border-border px-4 font-medium hover:bg-surface-2"
        >
          Xem tất cả lượt trong ngày
        </button>
      )}
      {all && list.length > shown && (
        <button
          type="button"
          onClick={() => {
            setShown((n) => n + VISITS_PAGE);
          }}
          className="mt-2 min-h-11 w-full rounded-[10px] border border-border px-4 font-medium hover:bg-surface-2"
        >
          Xem thêm ({list.length - shown} lượt nữa)
        </button>
      )}
    </section>
  );
}
