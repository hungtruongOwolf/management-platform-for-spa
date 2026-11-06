// Today's visits on /quay (full width, EM round 4 item 6; aligned table, EM round 5 item 1): one
// row per visit with fixed columns (Giờ, Khách, Dịch vụ · KTV, Thanh toán, Số tiền, ⋯). A click on
// the row (or the name, for keyboards) opens the details inline: lines, payment, phone, health
// warning, profile link, and the reason of a cancelled visit. "Sửa lượt" (also in the details) and
// "Hủy lượt" live in the ⋯ menu, shown only when the server would allow them. A
// lazy chunk: it renders after the first paint anyway (its query waits for it), so it stays off
// the startup bundle.
import type { Visit } from "@vpyd/api-client";
import {
  keepPreviousData,
  useQuery,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query";
import { Link, useNavigate } from "@tanstack/react-router";
import { formatVnd } from "@vpyd/ui";
import { Suspense, lazy, useState } from "react";
import { CustomerName } from "../../app/CustomerName";
import { DateField } from "../../components/DateField";
import { ResponsiveTable, type Column } from "../../components/ResponsiveTable";
import { RowMenu, type RowMenuItem } from "../../components/RowMenu";
import { useAfterFirstPaint } from "../../lib/afterFirstPaint";
import { messageFor } from "../../lib/errors";
import { invalidateManagementViews } from "../../lib/invalidate";
import { serviceLabel } from "../../lib/names";
import { can, meQuery } from "../auth/session";
import { mayCorrectVisit, spaToday } from "../corrections/api";
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

/** The "Thanh toán" cell: how it was paid, short (the amount has its own column). */
function paymentShort(v: Visit): string {
  // The "Đã hủy" badge sits by the name; this says what the cancel gave back.
  if (v.corrected_by_visit_id) return "Thay bằng lượt mới";
  if (v.status === "voided")
    return v.total > 0 ? "Đã hoàn tiền" : "Đã trả lại buổi";
  if (v.total === 0) return "Trừ liệu trình";
  const methods = new Set(
    v.payments.filter((p) => p.kind === "payment").map((p) => p.method),
  );
  if (methods.size === 0) return "Đã thanh toán";
  if (methods.size > 1) return "Tiền mặt + CK";
  return methods.has("cash") ? "Tiền mặt" : "Chuyển khoản";
}

/** Inline details of one visit: lines, technicians, payment, the customer's phone + a profile link. */
function VisitDetails({
  v,
  onCorrect,
}: {
  v: Visit;
  /** Shown only when the caller may correct this visit. */
  onCorrect: (() => void) | null;
}) {
  const customer = useQuery(customerQuery(v.customer_id));
  return (
    <div className="grid gap-2 rounded-[10px] bg-surface-2 p-3 text-sm">
      {v.corrected_by_visit_id ? (
        <p className="font-semibold">
          Đã sửa → lượt mới.{" "}
          <span className="font-normal text-text-muted">
            {v.void_reason ?? ""}
          </span>
        </p>
      ) : (
        v.status === "voided" && (
          <p className="font-semibold text-danger">
            Lý do hủy: {v.void_reason ?? "không ghi"}
          </p>
        )
      )}
      {v.corrects_visit_id && (
        <p className="text-text-muted">
          Lượt đã sửa (thay cho lượt ghi trước).
        </p>
      )}
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
        {onCorrect && (
          <button
            type="button"
            onClick={onCorrect}
            className="min-h-11 rounded-[10px] border border-border bg-surface px-3 font-semibold"
          >
            Sửa lượt
          </button>
        )}
      </p>
      {customer.data && <HealthNote customer={customer.data} />}
    </div>
  );
}

/**
 * Columns of the day's visits. Fixed widths keep every row on the same grid; the service text
 * wraps (at most two lines) inside its own column. Phones: cards (name, time · services, amount,
 * payment, ⋯).
 */
function visitColumns({
  expanded,
  toggle,
  actions,
}: {
  expanded: string | null;
  toggle: (id: string) => void;
  actions: (v: Visit) => RowMenuItem[];
}): Column<Visit>[] {
  return [
    {
      id: "time",
      header: "Giờ",
      width: "80px",
      card: "subtitle",
      className: "tabular-nums font-semibold whitespace-nowrap",
      cell: (v) => timeOf(v.check_in_at),
    },
    {
      id: "customer",
      header: "Khách",
      width: "24%",
      card: "title",
      cell: (v) => (
        <span className="flex flex-wrap items-center gap-x-2 gap-y-1">
          {/* The keyboard path of the row click: Enter/Space opens the details. */}
          <button
            type="button"
            className="row-toggle"
            aria-expanded={expanded === v.id}
            onClick={() => {
              toggle(v.id);
            }}
          >
            <CustomerName name={v.customer_name} gender={v.customer_gender} />
          </button>
          {v.corrected_by_visit_id ? (
            <span className="badge">Đã sửa</span>
          ) : (
            v.status === "voided" && (
              <span className="badge badge-danger">Đã hủy</span>
            )
          )}
        </span>
      ),
    },
    {
      id: "services",
      header: "Dịch vụ · KTV",
      cardLabel: "Dịch vụ",
      cell: (v) => {
        const services = v.lines
          .map(
            (l) =>
              `${serviceLabel(l.service_name, l.category_name)}${l.source === "package" ? " (trừ buổi)" : ""}`,
          )
          .join(", ");
        const staff = [...new Set(v.lines.map((l) => l.staff_name))].join(", ");
        return (
          <span className="grid min-w-0">
            <span className="line-clamp-2 text-ellipsis" title={services}>
              {services}
            </span>
            <span className="truncate text-text-muted">KTV {staff}</span>
          </span>
        );
      },
    },
    {
      id: "payment",
      header: "Thanh toán",
      width: "16%",
      cell: (v) => <span className="text-text-muted">{paymentShort(v)}</span>,
    },
    {
      id: "amount",
      header: "Số tiền",
      width: "112px",
      numeric: true,
      card: "aside",
      className: "visit-amount",
      cell: (v) => (v.total ? formatVnd(v.total) : "—"),
    },
    {
      id: "actions",
      header: <span className="sr-only">Thao tác</span>,
      width: "60px",
      card: "actions",
      className: "text-right",
      cell: (v) => (
        <RowMenu
          label={`Thao tác lượt ${timeOf(v.check_in_at)} của ${v.customer_name}`}
          items={actions(v)}
        />
      ),
    },
  ];
}

export function TodayVisits({
  onCorrect,
}: {
  /** "Sửa lượt": open the visit in the cart. */
  onCorrect?: (visitId: string) => void;
}) {
  const { data: me } = useSuspenseQuery(meQuery);
  const queryClient = useQueryClient();
  const navigate = useNavigate();
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
  const correctable = (v: Visit) =>
    onCorrect !== undefined &&
    mayCorrectVisit(me?.permissions ?? [], v, todayIso);
  const toggle = (id: string) => {
    setExpanded((open) => (open === id ? null : id));
    if (voiding === id) setVoiding(null);
  };
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
        <div className="rt-host overflow-hidden rounded-[10px] border border-border">
          <ResponsiveTable
            caption={isToday ? "Lượt khách hôm nay" : "Lượt khách trong ngày"}
            rows={visible}
            rowKey={(v) => v.id}
            cardsBelow={1024}
            rowClassName={(v) =>
              v.status === "voided" ? "visit-voided" : undefined
            }
            columns={visitColumns({
              expanded,
              toggle,
              actions: (v) => [
                {
                  label: "Mở hồ sơ khách",
                  onSelect: () => {
                    void navigate({
                      to: "/khach-hang/$customerId",
                      params: { customerId: v.customer_id },
                    });
                  },
                },
                ...(correctable(v)
                  ? [
                      {
                        label: "Sửa lượt",
                        onSelect: () => {
                          onCorrect?.(v.id);
                        },
                      },
                    ]
                  : []),
                ...(mayVoid && v.status === "completed"
                  ? [
                      {
                        label: "Hủy lượt",
                        danger: true,
                        onSelect: () => {
                          setNotice(null);
                          setExpanded(v.id);
                          setVoiding(v.id);
                        },
                      },
                    ]
                  : []),
              ],
            })}
            onRowClick={(v) => {
              toggle(v.id);
            }}
            expanded={(v) =>
              voiding === v.id ? (
                <Suspense
                  fallback={
                    <p role="status" className="text-text-muted">
                      Đang mở…
                    </p>
                  }
                >
                  <VoidVisitForm
                    visit={v}
                    onCancel={() => {
                      setVoiding(null);
                    }}
                    onDone={(voided) => {
                      setVoiding(null);
                      // Stay open on the cancelled visit: its details now carry the reason.
                      setExpanded(voided.id);
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
                </Suspense>
              ) : expanded === v.id ? (
                <VisitDetails
                  v={v}
                  onCorrect={
                    correctable(v)
                      ? () => {
                          onCorrect?.(v.id);
                        }
                      : null
                  }
                />
              ) : null
            }
          />
        </div>
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
