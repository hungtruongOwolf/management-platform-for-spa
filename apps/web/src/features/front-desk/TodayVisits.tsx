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
import { useNavigate } from "@tanstack/react-router";
import "./visits.css";
import { formatVnd } from "@vpyd/ui";
import { Suspense, lazy, useState } from "react";
import { CustomerName } from "../../app/CustomerName";
import { DateField } from "../../components/DateField";
import { ResponsiveTable, type Column } from "../../components/ResponsiveTable";
import { RowMenu, type RowMenuItem } from "../../components/RowMenu";
import { useAfterFirstPaint } from "../../lib/afterFirstPaint";
import { spaDateTime } from "../../lib/format";
import { messageFor, retryable } from "../../lib/errors";
import { invalidateManagementViews } from "../../lib/invalidate";
import { serviceLabel } from "../../lib/names";
import { can, meQuery } from "../auth/session";
import { mayCorrectVisit, spaToday } from "../corrections/api";
import { customerQuery } from "../customers/api";
import { HealthNote } from "../customers/HealthNote";
import { DaySales } from "./DaySales";
import {
  VISITS_PAGE,
  timeOf,
  todayAllVisitsQuery,
  todayVisitsQuery,
  visitsOfDayQuery,
} from "./api";

const VisitUpsell = lazy(() =>
  import("./VisitUpsell").then((m) => ({ default: m.VisitUpsell })),
);
const VoidVisitForm = lazy(() =>
  import("../corrections/VoidVisitForm").then((m) => ({
    default: m.VoidVisitForm,
  })),
);

type PaymentDto = Visit["payments"][number];

/** "Tiền mặt" / "Chuyển khoản" / "Tiền mặt + CK" of some payments; "" when none. */
function methodText(payments: PaymentDto[]): string {
  const methods = new Set(
    payments.filter((p) => p.kind === "payment").map((p) => p.method),
  );
  if (methods.size === 0) return "";
  if (methods.size > 1) return "Tiền mặt + CK";
  return methods.has("cash") ? "Tiền mặt" : "Chuyển khoản";
}

/** "Tiền mặt 20.000đ · CK 25.000đ" when paid both ways, else null (the row says the method). */
export function splitText(payments: PaymentDto[]): string | null {
  const paid = (method: PaymentDto["method"]) =>
    payments
      .filter((p) => p.kind === "payment" && p.method === method)
      .reduce((s, p) => s + p.amount, 0);
  const cash = paid("cash");
  const transfer = paid("bank_transfer");
  return cash > 0 && transfer > 0
    ? `Tiền mặt ${formatVnd(cash)} · CK ${formatVnd(transfer)}`
    : null;
}

type LineDto = Visit["lines"][number];

/** Who sold a one-off line ("Khách tự chọn" when nobody did). */
function referrerText(name: string | null | undefined): string {
  return `Giới thiệu: ${name ?? "Khách tự chọn"}`;
}

/** "còn 4/12" after a package session; "" when unknown. */
function sessionsLeft(l: LineDto): string {
  return l.sessions_left_after != null
    ? `còn ${String(l.sessions_left_after)}/${String(l.sessions_total ?? "?")}`
    : "";
}

/**
 * "Trừ buổi · còn 4/12" for a package session. A voided visit gave the session back, so its
 * snapshot count is no longer true (F15): "Trừ buổi · đã trả lại buổi".
 */
export function sessionText(v: Pick<Visit, "status">, l: LineDto): string {
  if (v.status === "voided") return "Trừ buổi · đã trả lại buổi";
  return `Trừ buổi${sessionsLeft(l) ? ` · ${sessionsLeft(l)}` : ""}`;
}

/** Packages sold in the visit that still stand (a voided visit keeps them, AC12.4). */
function soldPackages(v: Visit) {
  return v.package_sales.filter((p) => !p.voided);
}

/**
 * The "Số tiền" of a visit: everything collected in it — one-off lines after discount plus the
 * packages sold in it. A voided visit shows what its cancel gave back (the one-off part).
 */
export function visitAmount(v: Visit): number {
  if (v.status === "voided") return v.total;
  return v.total + soldPackages(v).reduce((s, p) => s + p.total, 0);
}

/** The "Thanh toán" cell: how it was paid, short (the amount has its own column). */
function paymentShort(v: Visit): string {
  // The "Đã hủy" badge sits by the name; this says what the cancel gave back.
  if (v.corrected_by_visit_id) return "Thay bằng lượt mới";
  if (v.status === "voided")
    return v.total > 0 ? "Đã hoàn tiền" : "Đã trả lại buổi";
  if (visitAmount(v) === 0) return "Trừ liệu trình";
  return (
    methodText([
      ...v.payments,
      ...soldPackages(v).flatMap((p) => p.payments),
    ]) || "Đã thanh toán"
  );
}

/**
 * One row of the details, on the table's own columns (time · customer · service · payment ·
 * amount · ⋯) so each amount sits under "Số tiền". Phones: service + amount, meta underneath.
 */
function DetailRow({
  who,
  main,
  meta,
  pay,
  amount,
  strong = false,
}: {
  who?: React.ReactNode;
  main: React.ReactNode;
  meta?: React.ReactNode;
  pay?: React.ReactNode;
  amount?: React.ReactNode;
  strong?: boolean;
}) {
  return (
    <div className={`vd-row ${strong ? "is-total" : ""}`}>
      <span className="vd-who">{who}</span>
      <span className="vd-main">
        {main}
        {meta && <span className="vd-meta">{meta}</span>}
      </span>
      <span className="vd-pay">{pay}</span>
      <span className="vd-amount">{amount}</span>
    </div>
  );
}

/**
 * The phone in a visit's details: "SĐT 0905 …", "Chưa có SĐT" for a named customer without one
 * (decision E), nothing for a nameless walk-in. Never a raw null (EM 2026-11-07).
 */
export function phoneLine(
  c: { phone?: string | null; named: boolean } | undefined,
): string | null {
  if (!c) return null;
  if (c.phone) return `SĐT ${c.phone}`;
  return c.named ? "Chưa có SĐT" : null;
}

/**
 * Inline details of one visit as a flush sub-row (EM 2026-11-06: no card in the table, amounts
 * under "Số tiền"): its lines with technician and referrer, the discount, the packages sold in it,
 * what was collected, companions, and the status note. Actions live in the row's ⋯ menu.
 */
function VisitDetails({ v }: { v: Visit }) {
  const customer = useQuery(customerQuery(v.customer_id));
  const packages = v.package_sales;
  const note = v.corrected_by_visit_id
    ? `Đã sửa, thay bằng lượt mới. ${v.void_reason ?? ""}`
    : v.status === "voided"
      ? `Lý do hủy: ${v.void_reason ?? "không ghi"}`
      : v.corrects_visit_id
        ? "Lượt đã sửa (thay cho lượt ghi trước)."
        : null;
  // Both days on the spa's calendar: the UTC date of `created_at` is a day behind before 07:00.
  const backdated = v.business_date < spaToday(new Date(v.created_at));
  const phone = phoneLine(customer.data);
  // A breakdown only when the visit has several parts; one line repeats the row (EM 2026-11-06).
  const breakdown =
    v.lines.length > 1 || packages.length > 0 || v.discount_amount > 0;
  const moneyItems =
    v.lines.filter((l) => l.source === "one_off").length +
    packages.length +
    (v.discount_amount > 0 ? 1 : 0);
  const split =
    v.status === "voided"
      ? null
      : splitText([
          ...v.payments,
          ...soldPackages(v).flatMap((p) => p.payments),
        ]);
  const only = v.lines[0];
  const single =
    !breakdown && only
      ? only.source === "one_off"
        ? referrerText(only.referrer_name)
        : sessionText(v, only)
      : null;
  return (
    <div className="visit-details" data-testid="visit-details">
      {note && (
        <DetailRow
          who={phone}
          main={
            <span
              className={
                v.status === "voided" && !v.corrected_by_visit_id
                  ? "text-danger"
                  : ""
              }
            >
              {note}
            </span>
          }
        />
      )}
      {breakdown ? (
        <>
          {v.lines.map((l, i) => (
            <DetailRow
              key={l.id}
              who={i === 0 && !note ? phone : null}
              main={serviceLabel(l.service_name, l.category_name)}
              meta={`KTV ${l.staff_name}${l.source === "one_off" ? ` · ${referrerText(l.referrer_name)}` : ""}`}
              pay={
                l.source === "package" ? sessionText(v, l) : "Dịch vụ lẻ"
              }
              amount={
                l.source === "one_off" ? formatVnd(l.unit_price * l.qty) : "—"
              }
            />
          ))}
          {v.discount_amount > 0 && (
            <DetailRow
              main="Giảm giá dịch vụ lẻ"
              meta={v.discount_reason ?? undefined}
              amount={`−${formatVnd(v.discount_amount)}`}
            />
          )}
          {packages.map((p) => (
            <DetailRow
              key={p.sale_id}
              main={p.description}
              meta={[
                `Người chốt: ${p.closer_name ?? "Khách tự chọn"}`,
                p.beneficiary_name ? `Mua cho ${p.beneficiary_name}` : "",
                p.voided ? "Hóa đơn gói đã hủy" : "",
              ]
                .filter(Boolean)
                .join(" · ")}
              pay={`Gói liệu trình${methodText(p.payments) ? ` · ${methodText(p.payments)}` : ""}`}
              amount={
                p.voided ? <s>{formatVnd(p.total)}</s> : formatVnd(p.total)
              }
            />
          ))}
          {moneyItems > 1 && (
            <DetailRow
              strong
              main={v.status === "voided" ? "Tổng đã hoàn" : "Tổng cộng"}
              pay={paymentShort(v)}
              amount={formatVnd(visitAmount(v))}
            />
          )}
        </>
      ) : (
        // One line, nothing sold, no discount: the row already shows the service, technician,
        // payment and amount, so the details carry only what the row does not.
        (single || (!note && phone)) && (
          <DetailRow
            who={note ? null : phone}
            main=""
            meta={single ?? undefined}
          />
        )
      )}
      {split && <DetailRow main="Thanh toán" meta={split} />}
      {v.note && <DetailRow main="Ghi chú buổi" meta={v.note} />}
      {/* F4: each package sold here, its own note (sent only to health-note readers). */}
      {packages
        .filter((p) => p.note)
        .map((p) => (
          <DetailRow
            key={`note-${p.sale_id}`}
            main="Ghi chú gói"
            meta={
              packages.length > 1
                ? `${p.description}: ${p.note ?? ""}`
                : (p.note ?? "")
            }
          />
        ))}
      {v.companions.length > 0 && (
        <DetailRow main="Người đi cùng" meta={v.companions.join(", ")} />
      )}
      {backdated && (
        <DetailRow
          main="Nhập bù"
          meta={`Ghi lúc ${spaDateTime(v.created_at)}`}
        />
      )}
      {customer.data?.health_note && (
        <div className="vd-row vd-wide">
          <HealthNote customer={customer.data} />
        </div>
      )}
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
            <span className="badge badge-muted">Đã sửa</span>
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
        // A package sold in the visit is named too (F3): its price is part of "Số tiền".
        const sold = soldPackages(v).map((p) => p.description);
        const services =
          v.lines
            .map(
              (l) =>
                `${serviceLabel(l.service_name, l.category_name)}${l.source === "package" ? " (trừ buổi)" : ""}`,
            )
            .join(", ") + (sold.length > 0 ? ` + bán gói ${sold.join(", ")}` : "");
        const staff = [...new Set(v.lines.map((l) => l.staff_name))].join(", ");
        return (
          <span className="grid min-w-0">
            <span className="line-clamp-2 text-ellipsis" title={services}>
              {services}
            </span>
            <span className="truncate text-text-muted">KTV {staff}</span>
            {v.note && (
              <span className="truncate text-text-muted" title={v.note}>
                Ghi chú: {v.note}
              </span>
            )}
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
      cell: (v) => (visitAmount(v) ? formatVnd(visitAmount(v)) : "—"),
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
  resetSignal = 0,
}: {
  /** "Sửa lượt": open the visit in the cart. */
  onCorrect?: (visitId: string) => void;
  /** Changes when the cart saved or was reset: an older notice here no longer applies. */
  resetSignal?: number;
}) {
  const { data: me } = useSuspenseQuery(meQuery);
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const todayIso = spaToday();
  const [day, setDay] = useState(todayIso);
  const [voiding, setVoiding] = useState<string | null>(null);
  const [expanded, setExpanded] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  // "Đã hủy lượt…" stayed on after the next save (EM live test): a cart save or reset clears it.
  const [seenSignal, setSeenSignal] = useState(resetSignal);
  if (seenSignal !== resetSignal) {
    setSeenSignal(resetSignal);
    setNotice(null);
  }
  /** The saved visit open for "Thêm dịch vụ / bán gói" (which also converts, Q-U1). */
  const [upsell, setUpsell] = useState<string | null>(null);
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
  // The title counts real visits (F14): a voided row, or the old row a "Sửa lượt" replaced, stays
  // in the list (struck through) but is counted apart.
  const voidedCount = list.filter((v) => v.status === "voided").length;
  const countText = hasMore
    ? `${String(VISITS_PAGE)}+`
    : `${String(list.length - voidedCount)}${voidedCount > 0 ? ` · ${String(voidedCount)} đã hủy / sửa` : ""}`;
  const visible = list.slice(0, all ? shown : VISITS_PAGE);
  // UX only: the server decides (403 day.locked for managers; owners may).
  const mayVoid =
    can(me, "record.void_same_day") &&
    (isToday || can(me, "record.void_closed_day"));
  const correctable = (v: Visit) =>
    onCorrect !== undefined &&
    mayCorrectVisit(me?.permissions ?? [], v, todayIso);
  // Round 6 (UX only, the server decides): adding to a saved visit needs the same rights as
  // recording one, a past day the owner's; converting also voids the one-off part.
  const pastOk = isToday || can(me, "record.void_closed_day");
  const mayAdd = (v: Visit) =>
    v.status === "completed" &&
    pastOk &&
    can(me, "visit.record") &&
    can(me, "payment.collect");
  const mayConvert = (v: Visit) =>
    mayAdd(v) &&
    can(me, "package.sell") &&
    can(me, "record.void_same_day") &&
    v.lines.some((l) => l.source === "one_off");
  const toggle = (id: string) => {
    setNotice(null);
    setExpanded((open) => (open === id ? null : id));
    if (voiding === id) setVoiding(null);
    if (upsell === id) setUpsell(null);
  };
  const openUpsell = (v: Visit) => {
    setNotice(null);
    setVoiding(null);
    setExpanded(v.id);
    setUpsell(v.id);
  };
  return (
    <section aria-labelledby="today-title" className="panel">
      {/* The day picker sits on the title's row (EM polish round 2): no label line above it. */}
      <div className="panel-head">
        <h2 id="today-title" className="panel-title">
          {isToday
            ? "Lượt khách hôm nay"
            : `Lượt khách ngày ${day.split("-").reverse().join("/")}`}
          {today.data ? ` (${countText})` : ""}
        </h2>
        <DateField
          label="Xem ngày"
          hideLabel
          value={day}
          max={todayIso}
          onChange={(iso) => {
            setDay(iso || todayIso);
            setNotice(null);
            setVoiding(null);
            setExpanded(null);
            setAll(false);
            setShown(VISITS_PAGE);
          }}
        />
      </div>
      {!isToday && can(me, "record.void_same_day") && !mayVoid && (
        <p className="text-sm text-text-muted">
          Ngày đã chốt: chỉ chủ spa hủy được lượt của ngày này.
        </p>
      )}
      {notice && (
        <p role="status" className="note text-text">
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
        <p role="alert" className="field-error">
          {messageFor(today.error)}{" "}
          {retryable(today.error) && (
            <button
              type="button"
              className="btn btn-ghost min-h-9 px-2"
              onClick={() => void today.refetch()}
            >
              Thử lại
            </button>
          )}
        </p>
      )}
      {today.data?.length === 0 && (
        <p className="text-text-muted">
          {isToday
            ? "Chưa có khách nào hôm nay. Lượt vừa lưu sẽ hiện ở đây."
            : "Ngày này không có lượt khách nào. Chọn ngày khác ở ô ngày phía trên."}
        </p>
      )}
      {today.data && today.data.length > 0 && (
        <div className="rt-host visits-host overflow-hidden rounded-md border border-border">
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
                          setNotice(null);
                          onCorrect?.(v.id);
                        },
                      },
                    ]
                  : []),
                ...(mayAdd(v)
                  ? [
                      {
                        label: "Thêm dịch vụ / bán gói",
                        onSelect: () => {
                          openUpsell(v);
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
              ) : upsell === v.id ? (
                <Suspense
                  fallback={
                    <p
                      role="status"
                      className="visit-inline-form text-text-muted"
                    >
                      Đang mở…
                    </p>
                  }
                >
                  <VisitUpsell
                    visit={v}
                    mayConvert={mayConvert(v)}
                    onCancel={() => {
                      setUpsell(null);
                    }}
                    onDone={(saved, summary) => {
                      setUpsell(null);
                      setExpanded(saved.id);
                      setNotice(summary);
                    }}
                  />
                </Suspense>
              ) : expanded === v.id ? (
                <VisitDetails v={v} />
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
          className="btn btn-secondary btn-block"
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
          className="btn btn-secondary btn-block"
        >
          Xem thêm ({list.length - shown} lượt nữa)
        </button>
      )}
      {painted && can(me, "sale.read") && can(me, "customer.read") && (
        <DaySales
          day={day}
          isToday={isToday}
          mayVoid={mayVoid}
          mayEditNote={
            mayVoid &&
            can(me, "package.sell") &&
            can(me, "customer.health_notes.write")
          }
          onNotice={setNotice}
        />
      )}
    </section>
  );
}
