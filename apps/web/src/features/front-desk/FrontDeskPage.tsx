import type {
  Appointment,
  Customer,
  CustomerSummary,
  Service,
  Visit,
} from "@vpyd/api-client";
import {
  useMutation,
  useQuery,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query";
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
import { messageFor } from "../../lib/errors";
import { can, meQuery } from "../auth/session";
import { categoriesQuery, servicesQuery } from "../catalog/api";
import { customerQuery } from "../customers/api";
import { CustomerSearch } from "../customers/CustomerSearch";
import { StickyActions } from "../../components/StickyActions";
import { completeAppointment } from "../appointments/complete";
import { activeStaffQuery, exceedsCap, parseAmount } from "../sales/api";
import {
  type CartLine,
  type DiscountDraft,
  type Gender,
  type PaymentMode,
  amountDue,
  discountPreview,
  inCart,
  paymentsFor,
  priceFor,
  recordVisit,
  savedSummary,
  serviceInstant,
  staffByUse,
  timeOf,
  todayVisitsQuery,
  visitBody,
} from "./api";
import { CustomerCard } from "./CustomerCard";
// The countdown ticks from just after the first paint (own chunk; its box is reserved meanwhile).
const DayCountdown = lazy(() =>
  import("./DayCountdown").then((m) => ({ default: m.DayCountdown })),
);
import { frontDeskSearch } from "./search";
import { spaToday } from "../corrections/api";

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

// Below the fold / after a line is added: own chunks, fetched right after the first paint so they
// are ready when needed (startup JS ≤ 125 KB gzip, frontend.md "Perf budget").
const loadTodayVisits = () => import("./TodayVisits");
const loadCartOptions = () => import("./CartOptions");
const TodayVisits = lazy(() =>
  loadTodayVisits().then((m) => ({ default: m.TodayVisits })),
);
const ServiceTime = lazy(() =>
  loadCartOptions().then((m) => ({ default: m.ServiceTime })),
);
const DiscountField = lazy(() =>
  loadCartOptions().then((m) => ({ default: m.DiscountField })),
);

const field = "min-h-11 rounded-[6px] border border-border bg-surface px-3";
const card = "rounded-[14px] border border-border bg-surface p-4 shadow-card";

// Placeholders for the lazy pieces until they are mounted after the first paint: the same boxes,
// so swapping them in shifts nothing (CLS budget).
const countdownBox = <span aria-hidden="true" className="block h-10 w-64" />;
const todayVisitsSkeleton = (
  <section aria-label="Lượt khách hôm nay" className={card}>
    <div aria-hidden="true" className="h-24 skeleton" />
  </section>
);

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

/**
 * /quay — the front desk. Two tabs: "Ghi lượt" (default; the walk-in fast path, design/front-desk-flows.md:
 * find the customer + Enter → "Trừ 1 buổi" → pick the technician → Lưu, Ctrl+Enter) and "Lịch hẹn"
 * (lazy calendar). Every amount shown is a preview; the server prices the visit and enforces every rule.
 */
export function FrontDeskPage() {
  const { data: me } = useSuspenseQuery(meQuery);
  const [search, setSearch] = useSearchState(frontDeskSearch);
  const queryClient = useQueryClient();
  // Everything below the search box waits for the first paint: Lighthouse's simulated LCP counts
  // every request and script that runs before that paint, lazy chunks included (frontend.md
  // "Perf budget"). The technician pickers need the staff only once a line is in the cart.
  const painted = useAfterFirstPaint();
  const staff = useQuery({ ...activeStaffQuery, enabled: painted });
  const today = useQuery({
    ...todayVisitsQuery,
    enabled: painted,
  });
  useEffect(() => {
    if (!painted) return;
    void loadTodayVisits();
    void loadCartOptions();
  }, [painted]);
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
  const startFromAppointment = (a: Appointment, prefilled: CartLine[]) => {
    if (!a.customer_id) return;
    select({
      id: a.customer_id,
      full_name: a.customer_name,
      phone: a.customer_phone.replace(/^\+84/, "0"),
      gender: a.customer_gender ?? null,
    });
    setFocusDeduct(false);
    setAppointment(a);
    setLines(prefilled.map((l) => ({ ...l, key: nextKey.current++ })));
    setSearch({ view: "ghi" }, { push: true });
    if (a.business_date !== todayIso) {
      setServiceDay(a.business_date);
      setServiceTime(timeOf(a.arrived_at ?? a.starts_at));
    }
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
        {painted ? (
          <Suspense fallback={countdownBox}>
            <DayCountdown owner={mayBackdate} />
          </Suspense>
        ) : (
          countdownBox
        )}
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
                <p className="mt-2 hidden text-xs text-text-muted sm:block">
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
                  <Suspense fallback={null}>
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
                  </Suspense>
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
                  <StickyActions label="Lưu lượt">
                    <button
                      type="button"
                      onClick={save}
                      disabled={mutation.isPending}
                      className="min-h-13 w-full rounded-[10px] bg-primary text-lg font-semibold text-on-primary disabled:opacity-60"
                    >
                      {mutation.isPending ? "Đang lưu…" : "Lưu lượt khách"}
                    </button>
                  </StickyActions>
                </div>
              )}
            </section>
          </div>

          {painted ? (
            <Suspense fallback={todayVisitsSkeleton}>
              <TodayVisits />
            </Suspense>
          ) : (
            todayVisitsSkeleton
          )}
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
