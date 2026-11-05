import type {
  Appointment,
  Customer,
  CustomerSummary,
  Service,
  Visit,
} from "@vpyd/api-client";
import {
  keepPreviousData,
  useMutation,
  useQuery,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { formatVnd } from "@vpyd/ui";
import {
  Suspense,
  lazy,
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";
import { useAfterFirstPaint } from "../../lib/afterFirstPaint";
import { ApiError } from "../../lib/api";
import { invalidateManagementViews } from "../../lib/invalidate";
import { serviceLabel } from "../../lib/names";
import { useSearchState } from "../../lib/searchState";
import { CustomerName } from "../../app/CustomerName";
import { DateField } from "../../components/DateField";
import { messageFor } from "../../lib/errors";
import { can, meQuery } from "../auth/session";
import { categoriesQuery, servicesQuery } from "../catalog/api";
import { customerQuery } from "../customers/api";
import { CustomerSearch } from "../customers/CustomerSearch";
import { HealthNote } from "../customers/HealthNote";
import { completeAppointment } from "../appointments/api";
import {
  activeStaffQuery,
  customerPackagesQuery,
  exceedsCap,
  parseAmount,
} from "../sales/api";
import {
  type CartLine,
  type DiscountDraft,
  type Gender,
  type PaymentMode,
  amountDue,
  byUseOrder,
  discountPreview,
  inCart,
  isUsable,
  paymentText,
  paymentsFor,
  priceFor,
  recordVisit,
  savedSummary,
  serviceInstant,
  staffByUse,
  timeOf,
  todayAllVisitsQuery,
  todayVisitsQuery,
  VISITS_PAGE,
  visitBody,
  visitsOfDayQuery,
} from "./api";
import { CustomerCard } from "./CustomerCard";
import { DayCountdown } from "./DayCountdown";
import { frontDeskSearch } from "./search";
import { spaToday } from "../corrections/api";

// The void form only opens on demand: its own chunk.
const VoidVisitForm = lazy(() =>
  import("../corrections/VoidVisitForm").then((m) => ({
    default: m.VoidVisitForm,
  })),
);

// The new-customer form (react-hook-form + zod) is only needed when no match is found, so it is a
// separate chunk: it stays off the front desk's first paint (frontend.md, perf budget).
const CustomerForm = lazy(() =>
  import("../customers/CustomerForm").then((m) => ({
    default: m.CustomerForm,
  })),
);

// The calendar is the second tab: its own chunk, fetched only when opened (LCP budget).
const AppointmentsCalendar = lazy(() =>
  import("../appointments/AppointmentsCalendar").then((m) => ({
    default: m.AppointmentsCalendar,
  })),
);

const field = "min-h-11 rounded-[6px] border border-border bg-surface px-3";
const card = "rounded-[14px] border border-border bg-surface p-4 shadow-card";

interface Toast {
  text: string;
  error: boolean;
}

function summaryOf(c: Customer): CustomerSummary {
  return {
    id: c.id,
    full_name: c.full_name,
    phone: c.phone,
    gender: c.gender ?? null,
  };
}

function isTyping(el: Element | null): boolean {
  return (
    el instanceof HTMLInputElement ||
    el instanceof HTMLSelectElement ||
    el instanceof HTMLTextAreaElement
  );
}

/** "HH:MM" now in the spa's zone. */
function nowHm(): string {
  return timeOf(new Date().toISOString());
}

/** ISO day `days` before `day`. */
function daysBefore(day: string, days: number): string {
  const d = new Date(`${day}T00:00:00Z`);
  d.setUTCDate(d.getUTCDate() - days);
  return d.toISOString().slice(0, 10);
}

/**
 * /quay — the front desk. Two tabs: "Ghi lượt" (default; the walk-in fast path, design/front-desk-flows.md:
 * find the customer + Enter → "Trừ 1 buổi" → pick the technician → Lưu, Ctrl+Enter) and "Lịch hẹn"
 * (lazy calendar). Every amount shown is a preview; the server prices the visit and enforces every rule.
 */
export function FrontDeskPage() {
  const { data: me } = useSuspenseQuery(meQuery);
  const [search, setSearch] = useSearchState(frontDeskSearch);
  const queryClient = useQueryClient();
  const staff = useQuery(activeStaffQuery);
  const painted = useAfterFirstPaint();
  // Secondary to the search box: fetched just after the first paint (see useAfterFirstPaint).
  const today = useQuery({
    ...todayVisitsQuery,
    enabled: painted,
  });
  const todayIso = spaToday();

  const [customer, setCustomer] = useState<CustomerSummary | null>(null);
  const [creating, setCreating] = useState<string | null>(null);
  const [lines, setLines] = useState<CartLine[]>([]);
  const [attempted, setAttempted] = useState(false);
  const [mode, setMode] = useState<PaymentMode>("cash");
  const [cashText, setCashText] = useState("");
  const [note, setNote] = useState("");
  const [toast, setToast] = useState<Toast | null>(null);
  const [focusDeduct, setFocusDeduct] = useState(false);
  /** The appointment this visit completes (calendar → "Hoàn tất & ghi lượt"); null = walk-in. */
  const [appointment, setAppointment] = useState<Appointment | null>(null);
  /** "Giờ làm": "" = now. `serviceDay` "" = today (only owners pick another day). */
  const [serviceTime, setServiceTime] = useState("");
  const [serviceDay, setServiceDay] = useState("");
  const [discount, setDiscount] = useState<DiscountDraft | null>(null);
  /** Line whose technician picker takes focus when it mounts (fast path step 2 → 3). */
  const focusLineKey = useRef<number | null>(null);
  // A fresh key per visit; kept across retries so a resend never records twice.
  const [idempotencyKey, setIdempotencyKey] = useState(() =>
    crypto.randomUUID(),
  );
  const [searchGeneration, setSearchGeneration] = useState(0);
  const searchInput = useRef<HTMLInputElement | null>(null);
  const nextKey = useRef(1);
  const staffSelects = useRef(new Map<number, HTMLSelectElement>());

  const showToast = useCallback((text: string, error = false) => {
    setToast({ text, error });
  }, []);
  useEffect(() => {
    if (!toast) return;
    const t = setTimeout(() => {
      setToast(null);
    }, 4000);
    return () => {
      clearTimeout(t);
    };
  }, [toast]);

  const due = amountDue(lines);
  const discountAmount = discountPreview(due, discount);
  const payable = due - discountAmount;
  const cashPart = parseAmount(cashText);
  const technicians = staffByUse(staff.data ?? [], today.data ?? []);
  const mayBackdate = can(me, "visit.backdate");

  const resetVisit = () => {
    setCustomer(null);
    setLines([]);
    setAttempted(false);
    setMode("cash");
    setCashText("");
    setNote("");
    setAppointment(null);
    setServiceTime("");
    setServiceDay("");
    setDiscount(null);
    setIdempotencyKey(crypto.randomUUID());
    setSearchGeneration((g) => g + 1);
    setTimeout(() => searchInput.current?.focus(), 0);
  };

  const mutation = useMutation({
    mutationFn: async (body: Parameters<typeof recordVisit>[0]) => {
      if (!appointment) return recordVisit(body, idempotencyKey);
      // eslint-disable-next-line @typescript-eslint/no-unused-vars
      const { customer_id, ...visit } = body;
      const done = await completeAppointment(
        appointment.id,
        visit,
        idempotencyKey,
      );
      return done.visit;
    },
    onSuccess: async (visit: Visit) => {
      queryClient.setQueryData<Visit[]>(todayVisitsQuery.queryKey, (old) =>
        visit.business_date === todayIso
          ? [visit, ...(old ?? []).filter((v) => v.id !== visit.id)]
          : old,
      );
      if (appointment)
        void queryClient.invalidateQueries({ queryKey: ["appointments"] });
      if (visit.business_date !== todayIso)
        void queryClient.invalidateQueries({
          queryKey: ["visits", visit.business_date],
        });
      await queryClient.invalidateQueries({
        queryKey: ["customer-packages", visit.customer_id],
      });
      void invalidateManagementViews(queryClient, visit.customer_id);
      showToast(
        savedSummary(visit) +
          (visit.business_date !== todayIso
            ? ` · nhập bù ngày ${visit.business_date.split("-").reverse().join("/")}`
            : ""),
      );
      resetVisit();
    },
    onError: async (error) => {
      if (error instanceof ApiError && error.status === 409 && customer) {
        // Another device used the session: show fresh counts, keep the other lines.
        await queryClient.invalidateQueries({
          queryKey: ["customer-packages", customer.id],
        });
      }
      showToast(messageFor(error), true);
    },
  });

  const discountProblem =
    discount !== null && (discountAmount <= 0 || discount.reason.trim() === "");
  const problems = {
    staff: lines.some((l) => l.staffId === ""),
    cash:
      payable > 0 &&
      mode === "mixed" &&
      (Number.isNaN(cashPart) || cashPart <= 0 || cashPart >= payable),
    time: serviceDay !== "" && serviceTime === "",
  };

  const save = () => {
    if (!customer || lines.length === 0 || mutation.isPending) return;
    setAttempted(true);
    if (problems.staff) {
      showToast("Mỗi dịch vụ cần chọn KTV thực hiện.", true);
      const missing = lines.find((l) => l.staffId === "");
      if (missing) staffSelects.current.get(missing.key)?.focus();
      return;
    }
    if (problems.cash) {
      showToast("Tiền mặt phải lớn hơn 0 và nhỏ hơn số phải trả.", true);
      return;
    }
    if (discountProblem) {
      showToast("Giảm giá cần mức giảm hợp lệ và lý do.", true);
      return;
    }
    if (problems.time) {
      showToast("Chọn giờ làm cho ngày nhập bù.", true);
      return;
    }
    mutation.mutate(
      visitBody(
        customer.id,
        lines,
        paymentsFor(payable, mode, cashPart),
        can(me, "customer.health_notes.write") ? note : "",
        {
          discount: due > 0 ? discount : null,
          checkInAt: serviceInstant(serviceDay || todayIso, serviceTime),
        },
      ),
    );
  };

  // Keyboard: "/" → search, Ctrl+Enter → save, Esc → drop the selected customer.
  const dropCustomer = () => {
    if (!customer) return;
    setCustomer(null);
    setLines([]);
    setCreating(null);
    setAppointment(null);
    setDiscount(null);
    searchInput.current?.focus();
  };
  const saveRef = useRef(save);
  const dropRef = useRef(dropCustomer);
  useEffect(() => {
    saveRef.current = save;
    dropRef.current = dropCustomer;
  });
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const typing = isTyping(document.activeElement);
      if (e.key === "/" && !typing) {
        e.preventDefault();
        searchInput.current?.focus();
      } else if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
        e.preventDefault();
        saveRef.current();
      } else if (e.key === "Escape" && !typing) {
        dropRef.current();
      }
    };
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("keydown", onKey);
    };
  }, []);

  const select = (c: CustomerSummary) => {
    setCreating(null);
    setCustomer(c);
    setLines([]);
    setAttempted(false);
    setDiscount(null);
    setAppointment(null);
    setIdempotencyKey(crypto.randomUUID());
    setFocusDeduct(true);
  };

  const addLine = (line: CartLine) => {
    setLines((ls) => [...ls, line]);
    focusLineKey.current = line.key;
  };

  /** Calendar → "Hoàn tất & ghi lượt": the cart filled with the booked services and technician. */
  const startFromAppointment = (a: Appointment) => {
    if (!a.customer_id) return;
    const summary: CustomerSummary = {
      id: a.customer_id,
      full_name: a.customer_name,
      phone: a.customer_phone.replace(/^\+84/, "0"),
      gender: a.customer_gender ?? null,
    };
    select(summary);
    setFocusDeduct(false);
    setAppointment(a);
    setSearch({ view: "ghi" }, { push: true });
    if (a.business_date !== todayIso) {
      setServiceDay(a.business_date);
      setServiceTime(timeOf(a.arrived_at ?? a.starts_at));
    }
    void Promise.all([
      queryClient.query(customerPackagesQuery(a.customer_id)),
      queryClient.query(servicesQuery(false)),
      queryClient.query(categoriesQuery),
    ])
      .then(([pkgs, services, cats]) => {
        const prefilled: CartLine[] = [];
        const usable = byUseOrder(pkgs).filter(isUsable);
        for (const s of a.services) {
          const pkg = usable.find((p) =>
            p.items.some(
              (i) =>
                i.service_id === s.service_id &&
                i.sessions_remaining - inCart(prefilled, i.id) > 0,
            ),
          );
          const item = pkg?.items.find(
            (i) =>
              i.service_id === s.service_id &&
              i.sessions_remaining - inCart(prefilled, i.id) > 0,
          );
          if (pkg && item) {
            prefilled.push({
              key: nextKey.current++,
              source: "package",
              serviceId: s.service_id,
              serviceName: item.name,
              categoryName: item.category_name,
              itemId: item.id,
              packageName: pkg.name,
              staffId: a.staff_id ?? "",
            });
            continue;
          }
          const service = services.find((x) => x.id === s.service_id);
          const price = service
            ? priceFor(service, summary.gender ?? null)
            : null;
          if (service && price?.ok)
            prefilled.push({
              key: nextKey.current++,
              source: "one_off",
              serviceId: service.id,
              serviceName: service.name,
              categoryName:
                cats.find((c) => c.id === service.category_id)?.name ??
                s.category_name,
              priceVariant: null,
              price: price.price,
              staffId: a.staff_id ?? "",
            });
        }
        setLines(prefilled);
      })
      .catch((e: unknown) => {
        showToast(messageFor(e), true);
      });
  };

  return (
    <div className="grid min-w-0 gap-4">
      <h1 className="sr-only">Màn hình quầy</h1>
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div role="group" aria-label="Màn hình quầy" className="segmented">
          <button
            type="button"
            aria-pressed={search.view === "ghi"}
            onClick={() => {
              setSearch({ view: "ghi" }, { push: true });
            }}
          >
            Ghi lượt khách
          </button>
          {can(me, "appointment.manage") && (
            <button
              type="button"
              aria-pressed={search.view === "lich"}
              onClick={() => {
                setSearch({ view: "lich" }, { push: true });
              }}
            >
              Lịch hẹn
            </button>
          )}
        </div>
        <DayCountdown owner={mayBackdate} />
      </div>

      {search.view === "lich" && can(me, "appointment.manage") ? (
        <Suspense
          fallback={
            <p role="status" className="text-text-muted">
              Đang mở lịch hẹn…
            </p>
          }
        >
          <AppointmentsCalendar onComplete={startFromAppointment} />
        </Suspense>
      ) : (
        <>
          <div className="grid gap-4 lg:grid-cols-[minmax(0,1.1fr)_minmax(0,1fr)]">
            <section
              aria-label="Tìm khách"
              className={`${card} grid min-w-0 content-start gap-3`}
            >
              <div>
                <CustomerSearch
                  key={searchGeneration}
                  inputRef={searchInput}
                  autoFocus
                  onSelect={select}
                  onCreate={(typed) => {
                    setCreating(typed);
                  }}
                />
                {creating !== null && can(me, "customer.write") && (
                  <div className="mt-3">
                    <Suspense
                      fallback={
                        <p role="status" className="text-text-muted">
                          Đang mở biểu mẫu…
                        </p>
                      }
                    >
                      <CustomerForm
                        typed={creating}
                        canWriteHealth={can(me, "customer.health_notes.write")}
                        onSaved={(c) => {
                          select(summaryOf(c));
                        }}
                        onOpenExisting={(id) => {
                          void queryClient
                            .query(customerQuery(id))
                            .then((c) => {
                              select(summaryOf(c));
                            })
                            .catch((e: unknown) => {
                              showToast(messageFor(e), true);
                            });
                        }}
                        onCancel={() => {
                          setCreating(null);
                        }}
                      />
                    </Suspense>
                  </div>
                )}
                <p className="mt-2 text-xs text-text-muted">
                  <kbd>↑</kbd> <kbd>↓</kbd> chọn, <kbd>Enter</kbd> mở,{" "}
                  <kbd>/</kbd> về ô tìm, <kbd>Ctrl</kbd>+<kbd>Enter</kbd> lưu,{" "}
                  <kbd>Esc</kbd> bỏ khách.
                </p>
              </div>
              {customer ? (
                <CustomerCard
                  key={customer.id}
                  summary={customer}
                  inCart={(itemId) => inCart(lines, itemId)}
                  canOverrideExpiry={can(me, "package.use_expired_override")}
                  focusFirst={focusDeduct}
                  onClear={dropCustomer}
                  onBlocked={(m) => {
                    showToast(m, true);
                  }}
                  onDeduct={({ pkg, item, expiryOverrideReason }) => {
                    setFocusDeduct(false);
                    addLine({
                      key: nextKey.current++,
                      source: "package",
                      serviceId: item.service_id,
                      serviceName: item.name,
                      categoryName: item.category_name,
                      itemId: item.id,
                      packageName: pkg.name,
                      staffId: appointment?.staff_id ?? "",
                      ...(expiryOverrideReason ? { expiryOverrideReason } : {}),
                    });
                  }}
                />
              ) : (
                <p className="text-center text-sm text-text-muted">
                  Chưa chọn khách. Tìm khách ở ô phía trên để bắt đầu một lượt.
                  Khách đặt trước: mở tab “Lịch hẹn”.
                </p>
              )}
            </section>

            <section aria-labelledby="cart-title" className={`${card} min-w-0`}>
              <h2 id="cart-title" className="mb-3 font-semibold">
                Lượt khách đang ghi
              </h2>
              {appointment && (
                <p className="mb-3 flex flex-wrap items-center justify-between gap-2 rounded-[10px] bg-info-soft p-2 text-sm text-info">
                  <span>
                    Hoàn tất lịch hẹn {timeOf(appointment.starts_at)} ·{" "}
                    {appointment.business_date.split("-").reverse().join("/")}
                  </span>
                  <button
                    type="button"
                    className="underline"
                    onClick={() => {
                      setAppointment(null);
                      setServiceDay("");
                      setServiceTime("");
                    }}
                  >
                    Ghi như khách vãng lai
                  </button>
                </p>
              )}
              {!customer && (
                <p className="text-text-muted">
                  Chọn khách để bắt đầu ghi lượt.
                </p>
              )}
              {customer && lines.length === 0 && (
                <p className="text-text-muted">
                  Chưa có dịch vụ. Bấm “Trừ 1 buổi” ở liệu trình hoặc thêm dịch
                  vụ lẻ.
                </p>
              )}
              {lines.length > 0 && (
                <ul aria-label="Dịch vụ trong lượt" className="grid gap-2">
                  {lines.map((l) => {
                    const invalid = attempted && l.staffId === "";
                    const selectId = `staff-${String(l.key)}`;
                    return (
                      <li
                        key={l.key}
                        className="grid grid-cols-[minmax(0,1fr)_auto] gap-2 rounded-[10px] border border-border px-3 py-2"
                      >
                        <div>
                          <strong>
                            {serviceLabel(l.serviceName, l.categoryName)}
                          </strong>
                          <span className="block text-xs text-text-muted">
                            {l.source === "package"
                              ? `Trừ liệu trình · ${l.packageName}${l.expiryOverrideReason ? " · cho dùng tiếp" : ""}`
                              : "Dịch vụ lẻ"}
                          </span>
                        </div>
                        <span className="text-right font-semibold tabular-nums">
                          {l.source === "one_off" ? formatVnd(l.price) : "—"}
                        </span>
                        <div className="col-span-2 flex flex-wrap items-center gap-2">
                          <label htmlFor={selectId} className="text-sm">
                            KTV
                          </label>
                          <select
                            id={selectId}
                            ref={(el) => {
                              if (!el) {
                                staffSelects.current.delete(l.key);
                                return;
                              }
                              staffSelects.current.set(l.key, el);
                              if (focusLineKey.current === l.key) {
                                focusLineKey.current = null;
                                el.focus();
                              }
                            }}
                            value={l.staffId}
                            aria-invalid={invalid}
                            aria-required="true"
                            aria-describedby={
                              invalid ? `${selectId}-error` : undefined
                            }
                            onChange={(e) => {
                              const staffId = e.target.value;
                              setLines((ls) =>
                                ls.map((x) =>
                                  x.key === l.key ? { ...x, staffId } : x,
                                ),
                              );
                            }}
                            className={`${field} ${invalid ? "border-danger" : ""}`}
                          >
                            <option value="">
                              {staff.isPending ? "Đang tải…" : "Chọn KTV…"}
                            </option>
                            {technicians.map((s) => (
                              <option key={s.id} value={s.id}>
                                {s.full_name}
                              </option>
                            ))}
                          </select>
                          {invalid && (
                            <span
                              id={`${selectId}-error`}
                              className="text-sm text-danger"
                            >
                              Cần chọn KTV
                            </span>
                          )}
                          <span className="flex-1" />
                          <button
                            type="button"
                            aria-label={`Bỏ dòng ${l.serviceName}`}
                            onClick={() => {
                              setLines((ls) =>
                                ls.filter((x) => x.key !== l.key),
                              );
                            }}
                            className="min-h-11 rounded-[10px] border border-border px-3 text-sm"
                          >
                            Bỏ
                          </button>
                        </div>
                      </li>
                    );
                  })}
                </ul>
              )}

              {customer && (
                <AddOneOff
                  customer={customer}
                  onAdd={(service, priceVariant, price, categoryName) => {
                    addLine({
                      key: nextKey.current++,
                      source: "one_off",
                      serviceId: service.id,
                      serviceName: service.name,
                      ...(categoryName ? { categoryName } : {}),
                      priceVariant,
                      price,
                      staffId: appointment?.staff_id ?? "",
                    });
                  }}
                  onProblem={(m) => {
                    showToast(m, true);
                  }}
                />
              )}

              {customer && lines.length > 0 && (
                <div className="mt-4 grid content-start gap-3">
                  <ServiceTime
                    mayBackdate={mayBackdate}
                    today={todayIso}
                    day={serviceDay}
                    time={serviceTime}
                    invalid={attempted && problems.time}
                    onDay={(d) => {
                      setServiceDay(d === todayIso ? "" : d);
                      if (d !== todayIso && serviceTime === "")
                        setServiceTime(nowHm());
                    }}
                    onTime={setServiceTime}
                  />
                  {due > 0 && can(me, "sale.discount") && (
                    <DiscountField
                      due={due}
                      draft={discount}
                      onChange={setDiscount}
                      overCap={
                        discountAmount > 0 &&
                        exceedsCap(
                          due,
                          discountAmount,
                          can(me, "sale.discount_unlimited"),
                        )
                      }
                    />
                  )}
                  <p
                    className="flex justify-between text-lg font-bold"
                    aria-live="polite"
                  >
                    <span>Phải trả</span>
                    <span className="tabular-nums">
                      {discountAmount > 0 && (
                        <s className="mr-2 text-sm font-normal text-text-muted">
                          {formatVnd(due)}
                        </s>
                      )}
                      {formatVnd(payable)}
                    </span>
                  </p>
                  {payable > 0 ? (
                    <fieldset className="grid content-start gap-2">
                      <legend className="text-sm font-semibold">
                        Thanh toán (thu đủ)
                      </legend>
                      <div className="flex flex-wrap gap-3">
                        {(
                          [
                            ["cash", "Tiền mặt"],
                            ["bank_transfer", "Chuyển khoản"],
                            ["mixed", "Tiền mặt + chuyển khoản"],
                          ] as const
                        ).map(([value, label]) => (
                          <label
                            key={value}
                            className="inline-flex min-h-11 items-center gap-2"
                          >
                            <input
                              type="radio"
                              name="visit-payment"
                              checked={mode === value}
                              onChange={() => {
                                setMode(value);
                              }}
                            />
                            {label}
                          </label>
                        ))}
                      </div>
                      {mode === "mixed" && (
                        <label className="block max-w-xs">
                          <span className="text-sm font-semibold">
                            Tiền mặt (đ)
                          </span>
                          <input
                            className={`${field} w-full`}
                            inputMode="numeric"
                            value={cashText}
                            onChange={(e) => {
                              setCashText(e.target.value);
                            }}
                          />
                          {!Number.isNaN(cashPart) &&
                            cashPart > 0 &&
                            cashPart < payable && (
                              <span className="block text-xs text-text-muted">
                                Chuyển khoản: {formatVnd(payable - cashPart)}
                              </span>
                            )}
                        </label>
                      )}
                    </fieldset>
                  ) : (
                    <p className="text-sm text-text-muted">
                      Lượt này chỉ trừ buổi liệu trình, không phát sinh tiền.
                    </p>
                  )}
                  {can(me, "customer.health_notes.write") && (
                    <label className="block">
                      <span className="text-sm font-semibold">
                        Ghi chú buổi (không bắt buộc)
                      </span>
                      <textarea
                        value={note}
                        onChange={(e) => {
                          setNote(e.target.value);
                        }}
                        rows={2}
                        maxLength={2000}
                        className="w-full rounded-[6px] border border-border bg-surface p-2"
                      />
                    </label>
                  )}
                  {mutation.isError &&
                    !(mutation.error instanceof ApiError) && (
                      <p
                        role="alert"
                        className="rounded-[10px] bg-danger-soft p-3 text-sm text-danger"
                      >
                        Chưa lưu được. Bấm Lưu để thử lại (không bị ghi đôi).
                      </p>
                    )}
                  <button
                    type="button"
                    onClick={save}
                    disabled={mutation.isPending}
                    className="min-h-13 w-full rounded-[10px] bg-primary text-lg font-semibold text-on-primary disabled:opacity-60"
                  >
                    {mutation.isPending ? "Đang lưu…" : "Lưu lượt khách"}
                  </button>
                </div>
              )}
            </section>
          </div>

          <TodayVisits />
        </>
      )}

      <div
        role="status"
        aria-live="polite"
        className={`pointer-events-none fixed bottom-[calc(var(--tabbar-h,0px)+24px)] left-1/2 z-30 max-w-[calc(100%-32px)] -translate-x-1/2 rounded-[10px] border-l-4 px-4 py-3 shadow-float transition-[opacity,translate] duration-200 ease-[var(--ease-out)] motion-reduce:transition-none ${toast ? "translate-y-0 opacity-100" : "translate-y-2 opacity-0"} ${toast?.error ? "border-danger-soft bg-danger text-surface" : "border-chrome-accent bg-chrome text-chrome-text"}`}
      >
        {toast?.text}
      </div>
    </div>
  );
}

/** "Giờ làm": now by default; the desk may pick an earlier time today, the owner an earlier day. */
function ServiceTime({
  mayBackdate,
  today,
  day,
  time,
  invalid,
  onDay,
  onTime,
}: {
  mayBackdate: boolean;
  today: string;
  day: string;
  time: string;
  invalid: boolean;
  onDay: (iso: string) => void;
  onTime: (hm: string) => void;
}) {
  const [open, setOpen] = useState(day !== "" || time !== "");
  if (!open)
    return (
      <p className="flex flex-wrap items-center gap-2 text-sm">
        <span>
          Giờ làm: <strong>bây giờ</strong>
        </span>
        <button
          type="button"
          className="underline"
          onClick={() => {
            setOpen(true);
          }}
        >
          Đổi giờ làm{mayBackdate ? " / nhập bù ngày cũ" : ""}
        </button>
      </p>
    );
  return (
    <fieldset className="grid gap-2 rounded-[10px] border border-border p-3">
      <legend className="px-1 text-sm font-semibold">Giờ làm dịch vụ</legend>
      <div className="flex flex-wrap items-end gap-3">
        {mayBackdate && (
          <DateField
            label="Ngày làm"
            value={day || today}
            min={daysBefore(today, 366)}
            max={today}
            onChange={(iso) => {
              onDay(iso || today);
            }}
            className="grid gap-1 text-sm"
          />
        )}
        <label className="grid gap-1 text-sm">
          <span>Giờ (để trống = bây giờ)</span>
          <input
            type="time"
            step={300}
            value={time}
            aria-invalid={invalid}
            onChange={(e) => {
              onTime(e.target.value);
            }}
            className={`${field} ${invalid ? "border-danger" : ""}`}
          />
        </label>
      </div>
      {day !== "" && day < today && (
        <p className="text-sm text-warning-text">
          Nhập bù: lượt, hóa đơn và tiền thu được ghi vào ngày{" "}
          {day.split("-").reverse().join("/")}.
        </p>
      )}
    </fieldset>
  );
}

/** Discount on the one-off lines: % or amount + reason (D10). The server computes and checks. */
function DiscountField({
  due,
  draft,
  overCap,
  onChange,
}: {
  due: number;
  draft: DiscountDraft | null;
  overCap: boolean;
  onChange: (d: DiscountDraft | null) => void;
}) {
  if (!draft)
    return (
      <button
        type="button"
        className="justify-self-start text-sm underline"
        onClick={() => {
          onChange({ kind: "percent", value: "", reason: "" });
        }}
      >
        + Giảm giá dịch vụ lẻ
      </button>
    );
  const amount = discountPreview(due, draft);
  return (
    <fieldset className="grid gap-2 rounded-[10px] border border-border p-3">
      <legend className="px-1 text-sm font-semibold">
        Giảm giá dịch vụ lẻ
      </legend>
      <div className="flex flex-wrap items-end gap-2">
        <div role="group" aria-label="Kiểu giảm" className="segmented">
          {(
            [
              ["percent", "%"],
              ["amount", "Số tiền"],
            ] as const
          ).map(([k, label]) => (
            <button
              key={k}
              type="button"
              aria-pressed={draft.kind === k}
              onClick={() => {
                onChange({ ...draft, kind: k, value: "" });
              }}
            >
              {label}
            </button>
          ))}
        </div>
        <label className="grid gap-1 text-sm">
          <span>
            {draft.kind === "percent" ? "Mức giảm (%)" : "Số tiền giảm (đ)"}
          </span>
          <input
            inputMode="numeric"
            value={draft.value}
            onChange={(e) => {
              onChange({ ...draft, value: e.target.value });
            }}
            className={`${field} w-32`}
          />
        </label>
        <label className="grid min-w-0 flex-1 gap-1 text-sm">
          <span>Lý do (bắt buộc)</span>
          <input
            value={draft.reason}
            maxLength={500}
            onChange={(e) => {
              onChange({ ...draft, reason: e.target.value });
            }}
            className={`${field} w-full`}
          />
        </label>
        <button
          type="button"
          className="min-h-11 rounded-[10px] border border-border px-3 text-sm"
          onClick={() => {
            onChange(null);
          }}
        >
          Bỏ giảm giá
        </button>
      </div>
      <p className="text-sm" aria-live="polite">
        {amount > 0 ? `Giảm ${formatVnd(amount)}` : "Nhập mức giảm hợp lệ."}
        {overCap && (
          <span className="block text-danger">
            Vượt mức quản lý được giảm: cần chủ spa.
          </span>
        )}
      </p>
    </fieldset>
  );
}

/** One-off service: grouped by category, ★ first; price by the customer's gender. */
function AddOneOff({
  customer,
  onAdd,
  onProblem,
}: {
  customer: CustomerSummary;
  onAdd: (
    service: Service,
    priceVariant: Gender | null,
    price: number,
    categoryName: string | undefined,
  ) => void;
  onProblem: (message: string) => void;
}) {
  const services = useQuery(servicesQuery(false));
  const categories = useQuery(categoriesQuery);
  const [serviceId, setServiceId] = useState("");
  const [chosenGender, setChosenGender] = useState<Gender | null>(null);
  const known = customer.gender ?? null;
  const gender = known ?? chosenGender;

  const groups = (categories.data ?? []).map((c) => ({
    category: c,
    items: (services.data ?? [])
      .filter((s) => s.category_id === c.id)
      .sort((a, b) => Number(b.recommended) - Number(a.recommended)),
  }));

  const add = () => {
    const service = services.data?.find((s) => s.id === serviceId);
    if (!service) {
      onProblem("Chọn một dịch vụ trước.");
      return;
    }
    const r = priceFor(service, gender);
    if (!r.ok) {
      onProblem(
        r.reason === "need_gender"
          ? "Chọn Nữ hoặc Nam để tính giá dịch vụ này."
          : r.reason === "not_offered"
            ? `“${service.name}” không áp dụng cho khách ${gender === "male" ? "Nam" : "Nữ"}.`
            : `“${service.name}” chưa có giá.`,
      );
      return;
    }
    onAdd(
      service,
      known ? null : r.variant === "all" ? null : r.variant,
      r.price,
      categories.data?.find((c) => c.id === service.category_id)?.name,
    );
    setServiceId("");
  };

  const priceLabel = (s: Service) =>
    s.prices.all != null
      ? formatVnd(s.prices.all)
      : `Nữ ${s.prices.female != null ? formatVnd(s.prices.female) : "—"} / Nam ${s.prices.male != null ? formatVnd(s.prices.male) : "—"}`;

  return (
    <div className="mt-4 border-t border-border pt-3">
      <h3 className="mb-2 font-semibold">Thêm dịch vụ lẻ / dùng thêm</h3>
      {services.isError && (
        <p role="alert" className="text-sm text-danger">
          {messageFor(services.error)}
        </p>
      )}
      <div className="flex flex-wrap items-center gap-2">
        <label htmlFor="one-off-service" className="sr-only">
          Dịch vụ lẻ
        </label>
        <select
          id="one-off-service"
          value={serviceId}
          onChange={(e) => {
            setServiceId(e.target.value);
          }}
          // A definite width: a select's intrinsic width is its longest option, which would
          // otherwise widen the whole column past the viewport.
          className={`${field} w-56 min-w-56 flex-1`}
        >
          <option value="">
            {services.isPending ? "Đang tải bảng giá…" : "Chọn dịch vụ…"}
          </option>
          {groups
            .filter((g) => g.items.length > 0)
            .map((g) => (
              <optgroup key={g.category.id} label={g.category.name}>
                {g.items.map((s) => (
                  <option key={s.id} value={s.id}>
                    {s.recommended ? "★ " : ""}
                    {s.name} · {priceLabel(s)}
                  </option>
                ))}
              </optgroup>
            ))}
        </select>
        {!known && (
          <span
            role="group"
            aria-label="Giới tính để tính giá"
            className="inline-flex gap-1"
          >
            {(
              [
                ["female", "Nữ"],
                ["male", "Nam"],
              ] as const
            ).map(([g, label]) => (
              <button
                key={g}
                type="button"
                aria-pressed={chosenGender === g}
                onClick={() => {
                  setChosenGender(g);
                }}
                className="min-h-11 rounded-[10px] border border-border px-3 aria-pressed:border-primary aria-pressed:bg-primary-soft aria-pressed:text-primary"
              >
                {label}
              </button>
            ))}
          </span>
        )}
        <button
          type="button"
          onClick={add}
          className="min-h-11 rounded-[10px] border border-border px-4 font-medium hover:bg-surface-2"
        >
          Thêm
        </button>
      </div>
      {!known && (
        <p className="mt-1 text-xs text-text-muted">
          Khách chưa có giới tính: chọn Nữ/Nam để tính giá.
        </p>
      )}
    </div>
  );
}

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

function TodayVisits() {
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
