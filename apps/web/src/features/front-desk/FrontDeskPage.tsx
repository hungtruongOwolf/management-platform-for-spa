import type {
  Appointment,
  Customer,
  CustomerSummary,
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
import { fieldMessage, messageFor } from "../../lib/errors";
import { can, meQuery } from "../auth/session";
import { customerQuery } from "../customers/api";
import { CustomerSearch } from "../customers/CustomerSearch";
import { Icon } from "../../app/Icon";
import { completeAppointment } from "../appointments/complete";
import { activeStaffQuery, exceedsCap, parseAmount } from "../sales/api";
import {
  type CartLine,
  type DiscountDraft,
  type PaymentMode,
  amountDue,
  discountPreview,
  inCart,
  paymentsFor,
  recordVisit,
  savedSummary,
  serviceInstant,
  staffByUse,
  timeOf,
  todayVisitsQuery,
  visitBody,
} from "./api";
import { CustomerCard } from "./CustomerCard";
import { Tip } from "./Tip";
import "./front-desk.css";
import type {
  CorrectionDraft,
  CorrectionPlan,
  CorrectionSaved,
  CorrectionStart,
} from "./correction";
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
const AddOneOff = lazy(() =>
  loadCartOptions().then((m) => ({ default: m.AddOneOff })),
);
// "Sửa lượt" (US6b): only when a recorded visit is opened for correction.
const loadCorrection = () => import("./correction");
const CorrectionPanel = lazy(() =>
  import("./CorrectionPanel").then((m) => ({ default: m.CorrectionPanel })),
);

// Placeholders for the lazy pieces until they are mounted after the first paint: the same boxes,
// so swapping them in shifts nothing (CLS budget).
const countdownBox = <span aria-hidden="true" className="day-clock" />;
const todayVisitsSkeleton = (
  <section aria-label="Lượt khách hôm nay" className="panel">
    <div aria-hidden="true" className="h-24 skeleton" />
  </section>
);

// The one-off picker's box (label + field) while its chunk loads: nothing shifts when it lands.
const addOneOffBox = <div aria-hidden="true" className="h-[78px]" />;

/** "yyyy-mm-dd" → "dd/mm/yyyy". */
const dmyOf = (iso: string) => iso.split("-").reverse().join("/");

/** "+84905111222" → "0905 111 222" (the booking carries E.164). */
function localPhone(e164: string): string {
  const d = e164.replace(/^\+84/, "0");
  return /^\d{10}$/.test(d)
    ? `${d.slice(0, 4)} ${d.slice(4, 7)} ${d.slice(7)}`
    : d;
}

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
  /** The appointment this visit completes (calendar → "Khách đến · ghi lượt"); null = walk-in. */
  const [appointment, setAppointment] = useState<Appointment | null>(null);
  /** "Giờ làm": "" = now. `serviceDay` "" = today (only owners pick another day). */
  const [serviceTime, setServiceTime] = useState("");
  const [serviceDay, setServiceDay] = useState("");
  const [discount, setDiscount] = useState<DiscountDraft | null>(null);
  /** "Sửa lượt": the recorded visit this cart corrects (null = a new visit). */
  const [correcting, setCorrecting] = useState<CorrectionStart | null>(null);
  const [reason, setReason] = useState("");
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
    setCorrecting(null);
    setReason("");
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
      // Walk-ins are on the calendar too (spec appointments.md §10.1), not only completed bookings.
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

  const correction = useMutation({
    mutationFn: async (
      plan: Exclude<CorrectionPlan, { kind: "none" }>,
    ): Promise<CorrectionSaved> => {
      if (!correcting) throw new Error("no visit to correct");
      const m = await loadCorrection();
      return m.saveCorrection(correcting.visit, plan, idempotencyKey);
    },
    onSuccess: (saved) => {
      const old = correcting?.visit;
      void queryClient.invalidateQueries({ queryKey: ["visits"] });
      void queryClient.invalidateQueries({ queryKey: ["appointments"] });
      void queryClient.invalidateQueries({ queryKey: ["customer-packages"] });
      void invalidateManagementViews(queryClient, saved.visit.customer_id);
      if (old && old.customer_id !== saved.visit.customer_id)
        void invalidateManagementViews(queryClient, old.customer_id);
      showToast(saved.summary);
      resetVisit();
    },
    onError: (error) => {
      if (error instanceof ApiError && error.status === 409) {
        void queryClient.invalidateQueries({ queryKey: ["visits"] });
        void queryClient.invalidateQueries({ queryKey: ["customer-packages"] });
      }
      showToast(
        fieldMessage(error, "check_in_at") ??
          fieldMessage(error, "reason") ??
          messageFor(error),
        true,
      );
    },
  });
  const saving = mutation.isPending || correction.isPending;
  const correctionDraft: CorrectionDraft | null =
    correcting && customer
      ? {
          customerId: customer.id,
          lines,
          payments: paymentsFor(payable, mode, cashPart),
          discount: due > 0 ? discount : null,
          checkInAt: serviceInstant(serviceDay || todayIso, serviceTime),
          note,
          canWriteNote: can(me, "customer.health_notes.write"),
          reason,
        }
      : null;
  /** Sessions the visit being corrected holds: given back before the correction re-uses them. */
  const heldByCorrected = (itemId: string) =>
    correcting && customer?.id === correcting.visit.customer_id
      ? correcting.visit.lines
          .filter((l) => l.customer_package_item_id === itemId)
          .reduce((n, l) => n + l.qty, 0)
      : 0;

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
    if (!customer || lines.length === 0 || saving) return;
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
    if (correcting && correctionDraft) {
      const old = correcting;
      void loadCorrection().then((m) => {
        const plan = m.planCorrection(old.visit, old.note, correctionDraft);
        if (plan.kind === "none") {
          showToast("Chưa có gì thay đổi so với lượt đã ghi.", true);
          return;
        }
        if (plan.kind === "correct" && plan.body.reason === "") {
          showToast("Cần ghi lý do sửa lượt.", true);
          return;
        }
        correction.mutate(plan);
      });
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

  /** Calendar → "Khách đến · ghi lượt": the cart filled with the booked services and technician. */
  const startFromAppointment = (a: Appointment, prefilled: CartLine[]) => {
    if (!a.customer_id) return;
    select({
      id: a.customer_id,
      full_name: a.customer_name,
      phone: localPhone(a.customer_phone),
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

  /** "Sửa lượt": the cart filled with the recorded visit; the desk changes only what is wrong. */
  const startCorrection = (c: CorrectionStart) => {
    select(c.customer);
    setFocusDeduct(false);
    setLines(c.lines.map((l) => ({ ...l, key: nextKey.current++ })));
    setMode(c.mode);
    setCashText(c.cashText);
    setDiscount(c.discount);
    setServiceDay(c.serviceDay);
    setServiceTime(c.serviceTime);
    setNote(c.note);
    setReason("");
    setCorrecting(c);
    setTimeout(() => {
      const title = document.getElementById("cart-title");
      // jsdom has no scrollIntoView.
      if (title && "scrollIntoView" in title)
        title.scrollIntoView({ block: "start" });
    }, 0);
  };
  // `/quay?sua=<visit>` (the ⋯ menu of the day list, a customer's history) opens that visit.
  const startCorrectionRef = useRef(startCorrection);
  useEffect(() => {
    startCorrectionRef.current = startCorrection;
  });
  const requested = search.sua;
  useEffect(() => {
    if (!requested || !painted) return;
    setSearch({ sua: "" });
    void loadCorrection()
      .then((m) => m.loadCorrection(queryClient, requested, todayIso))
      .then((c) => {
        if (c.visit.status !== "completed") {
          showToast(
            c.visit.corrected_by_visit_id
              ? "Lượt này đã được sửa. Mở lượt mới nhất để sửa tiếp."
              : "Lượt này đã bị hủy, không sửa được.",
            true,
          );
          return;
        }
        startCorrectionRef.current(c);
      })
      .catch((e: unknown) => {
        showToast(messageFor(e), true);
      });
  }, [requested, painted, queryClient, todayIso, setSearch, showToast]);

  const shortcuts = (
    <Tip label="Phím tắt" align="right">
      <dl className="kbd-list">
        <dt>
          <kbd>↑</kbd> <kbd>↓</kbd> <kbd>Enter</kbd>
        </dt>
        <dd>chọn khách trong kết quả</dd>
        <dt>
          <kbd>/</kbd>
        </dt>
        <dd>về ô tìm khách</dd>
        <dt>
          <kbd>Ctrl</kbd> + <kbd>Enter</kbd>
        </dt>
        <dd>lưu lượt khách</dd>
        <dt>
          <kbd>Esc</kbd>
        </dt>
        <dd>bỏ khách đang chọn</dd>
      </dl>
    </Tip>
  );

  return (
    <div className="desk">
      <h1 className="sr-only">Màn hình quầy</h1>
      <div className="desk-toolbar">
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
          <div className="desk-grid">
            <section aria-label="Tìm khách" className="panel">
              <CustomerSearch
                key={searchGeneration}
                inputRef={searchInput}
                autoFocus
                aside={shortcuts}
                onSelect={select}
                onCreate={(typed) => {
                  setCreating(typed);
                }}
              />
              {creating !== null && can(me, "customer.write") && (
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
              )}
              {customer ? (
                <CustomerCard
                  key={customer.id}
                  summary={customer}
                  inCart={(itemId) =>
                    inCart(lines, itemId) - heldByCorrected(itemId)
                  }
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
                <p className="desk-empty">
                  Chưa chọn khách. Tìm theo SĐT hoặc tên ở ô phía trên; khách
                  đặt trước thì mở tab “Lịch hẹn”.
                </p>
              )}
            </section>

            <section aria-labelledby="cart-title" className="panel">
              <h2 id="cart-title" className="panel-title scroll-mt-20">
                {correcting ? "Sửa lượt khách" : "Lượt khách đang ghi"}
              </h2>
              {correcting && (
                <div className="context-bar">
                  <span>
                    Đang sửa lượt {timeOf(correcting.visit.check_in_at)} ·{" "}
                    {dmyOf(correcting.visit.business_date)} của{" "}
                    {correcting.visit.customer_name}
                  </span>
                  <button
                    type="button"
                    className="btn btn-ghost min-h-9 px-3"
                    onClick={resetVisit}
                  >
                    Bỏ sửa
                  </button>
                </div>
              )}
              {appointment && (
                <div className="context-bar">
                  <span>
                    Đang ghi cho lịch hẹn {timeOf(appointment.starts_at)}
                    {appointment.business_date !== todayIso
                      ? ` · ${dmyOf(appointment.business_date)}`
                      : ""}{" "}
                    · {appointment.customer_name}
                  </span>
                  <details
                    className="context-menu"
                    onKeyDown={(e) => {
                      const d = e.currentTarget;
                      if (e.key === "Escape" && d.open) {
                        e.stopPropagation();
                        d.open = false;
                        d.querySelector("summary")?.focus();
                      }
                    }}
                  >
                    <summary aria-label="Tùy chọn lịch hẹn">
                      <Icon name="dots" />
                    </summary>
                    <div className="context-menu-panel">
                      <button
                        type="button"
                        onClick={() => {
                          setAppointment(null);
                          setServiceDay("");
                          setServiceTime("");
                        }}
                      >
                        Bỏ liên kết lịch hẹn
                      </button>
                      <p>
                        Lịch hẹn vẫn mở (chưa hoàn tất). Lượt này được ghi
                        riêng, như khách đến không hẹn.
                      </p>
                    </div>
                  </details>
                </div>
              )}
              {!customer && (
                <p className="desk-empty">Chọn khách để bắt đầu ghi lượt.</p>
              )}
              {customer && lines.length === 0 && (
                <p className="desk-empty">
                  Chưa có dịch vụ. Bấm “Trừ 1 buổi” ở gói liệu trình, hoặc chọn
                  dịch vụ lẻ bên dưới.
                </p>
              )}
              {lines.length > 0 && (
                <ul aria-label="Dịch vụ trong lượt" className="cart-lines">
                  {lines.map((l) => {
                    const invalid = attempted && l.staffId === "";
                    const selectId = `staff-${String(l.key)}`;
                    return (
                      <li key={l.key} className="cart-line">
                        <div className="min-w-0">
                          <span className="cart-line-name">
                            {serviceLabel(l.serviceName, l.categoryName)}
                          </span>
                          <span className="cart-line-source">
                            {l.source === "package"
                              ? `Trừ liệu trình · ${l.packageName}${l.expiryOverrideReason ? " · cho dùng tiếp" : ""}`
                              : "Dịch vụ lẻ"}
                          </span>
                        </div>
                        <span className="cart-line-price">
                          {l.source === "one_off" ? formatVnd(l.price) : ""}
                        </span>
                        <button
                          type="button"
                          aria-label={`Bỏ dòng ${l.serviceName}`}
                          onClick={() => {
                            setLines((ls) => ls.filter((x) => x.key !== l.key));
                          }}
                          className="btn btn-ghost btn-icon text-text-muted"
                        >
                          <Icon name="x" />
                        </button>
                        <div className="cart-line-staff">
                          <label htmlFor={selectId}>KTV</label>
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
                            className="input"
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
                              className="field-error"
                            >
                              Cần chọn KTV
                            </span>
                          )}
                        </div>
                      </li>
                    );
                  })}
                </ul>
              )}

              {customer && (
                <Suspense fallback={addOneOffBox}>
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
                </Suspense>
              )}

              {customer && lines.length > 0 && (
                <>
                  <div className="cart-options">
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
                  </div>
                  {payable > 0 && (
                    <fieldset className="grid gap-2">
                      <legend className="section-label mb-2">
                        Thanh toán (thu đủ)
                      </legend>
                      <div className="pay-choices">
                        {(
                          [
                            ["cash", "Tiền mặt"],
                            ["bank_transfer", "Chuyển khoản"],
                            ["mixed", "Tiền mặt + CK"],
                          ] as const
                        ).map(([value, label]) => (
                          <label key={value} className="pay-choice">
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
                        <label className="field max-w-xs">
                          <span>Tiền mặt (đ)</span>
                          <input
                            className="input"
                            inputMode="numeric"
                            value={cashText}
                            aria-invalid={attempted && problems.cash}
                            onChange={(e) => {
                              setCashText(e.target.value);
                            }}
                          />
                          {!Number.isNaN(cashPart) &&
                            cashPart > 0 &&
                            cashPart < payable && (
                              <span className="field-hint">
                                Chuyển khoản: {formatVnd(payable - cashPart)}
                              </span>
                            )}
                        </label>
                      )}
                    </fieldset>
                  )}
                  {can(me, "customer.health_notes.write") && (
                    <label className="field">
                      <span>
                        Ghi chú buổi{" "}
                        <span className="font-normal text-text-muted">
                          (không bắt buộc)
                        </span>
                      </span>
                      <textarea
                        value={note}
                        onChange={(e) => {
                          setNote(e.target.value);
                        }}
                        rows={2}
                        maxLength={2000}
                        className="input"
                      />
                    </label>
                  )}
                  {correcting && correctionDraft && (
                    <Suspense fallback={null}>
                      <CorrectionPanel
                        visit={correcting.visit}
                        oldNote={correcting.note}
                        draft={correctionDraft}
                        invalid={attempted && reason.trim() === ""}
                        onReason={setReason}
                      />
                    </Suspense>
                  )}
                  {(mutation.isError || correction.isError) &&
                    !(mutation.error instanceof ApiError) &&
                    !(correction.error instanceof ApiError) && (
                      <p role="alert" className="alert">
                        Chưa lưu được. Bấm Lưu để thử lại (không bị ghi đôi).
                      </p>
                    )}
                  <div role="group" aria-label="Lưu lượt" className="checkout">
                    <p className="checkout-total" aria-live="polite">
                      <span>Phải trả</span>
                      <span className="checkout-amount">
                        {discountAmount > 0 && <s>{formatVnd(due)}</s>}
                        {formatVnd(payable)}
                      </span>
                    </p>
                    {payable === 0 && (
                      <p className="-mt-2 text-sm text-text-muted">
                        Lượt này chỉ trừ buổi liệu trình, không phát sinh tiền.
                      </p>
                    )}
                    <button
                      type="button"
                      onClick={save}
                      aria-disabled={saving}
                      aria-busy={saving}
                      className="btn btn-primary btn-lg btn-block"
                    >
                      {saving
                        ? "Đang lưu…"
                        : correcting
                          ? "Lưu sửa lượt"
                          : "Lưu lượt khách"}
                    </button>
                  </div>
                </>
              )}
            </section>
          </div>

          {painted ? (
            <Suspense fallback={todayVisitsSkeleton}>
              <TodayVisits
                onCorrect={(id) => {
                  setSearch({ sua: id });
                }}
              />
            </Suspense>
          ) : (
            todayVisitsSkeleton
          )}
        </>
      )}

      <div
        role="status"
        aria-live="polite"
        className={`desk-toast ${toast ? "is-shown" : ""} ${toast?.error ? "is-error" : ""}`}
      >
        {toast?.text}
      </div>
    </div>
  );
}
