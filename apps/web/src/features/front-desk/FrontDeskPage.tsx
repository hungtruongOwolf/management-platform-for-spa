import type {
  Appointment,
  Customer,
  CustomerSummary,
  Visit,
  VisitPackageSaleCreate,
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
import { useSearchState } from "../../lib/searchState";
import { fieldMessage, messageFor } from "../../lib/errors";
import { can, meQuery } from "../auth/session";
import { customerQuery } from "../customers/api";
import { CustomerSearch } from "../customers/CustomerSearch";
import { completeAppointment } from "../appointments/complete";
import { activeStaffQuery, exceedsCap, parseAmount } from "../sales/api";
import { boughtForOtherText } from "../sales/saleText";
import {
  type CartLine,
  type DiscountDraft,
  type PackageSaleDraft,
  type PaymentMode,
  amountDue,
  coverage,
  discountPreview,
  inCart,
  packagesDue,
  paymentsFor,
  recordVisit,
  savedSummary,
  serviceInstant,
  staffByUse,
  timeOf,
  todayVisitsQuery,
  visitBody,
} from "./api";
import type { CartDraft } from "./draftCart";
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
const LeaveCartConfirm = lazy(() =>
  loadCartOptions().then((m) => ({ default: m.LeaveCartConfirm })),
);
const WalkInInfo = lazy(() =>
  loadCartOptions().then((m) => ({ default: m.WalkInInfo })),
);
const CartContextBars = lazy(() =>
  loadCartOptions().then((m) => ({ default: m.CartContextBars })),
);
// Round 6: packages sold in the checkout (spec §10.4.1), and the unsaved-cart
// draft (Q-U8). Prefetched after the first paint, never on the startup path.
const loadCartPackages = () => import("./CartPackages");
const PackageSales = lazy(() =>
  loadCartPackages().then((m) => ({ default: m.PackageSales })),
);
const CartLines = lazy(() =>
  loadCartOptions().then((m) => ({ default: m.CartLines })),
);
const PaymentChoices = lazy(() =>
  loadCartOptions().then((m) => ({ default: m.PaymentChoices })),
);
const DraftKeeper = lazy(() =>
  loadCartPackages().then((m) => ({ default: m.DraftKeeper })),
);
// "Sửa lượt" (US6b): only when a recorded visit is opened for correction.
const loadCorrection = () => import("./correction");
const loadCorrectionPanel = () => import("./CorrectionPanel");
const CorrectionPanel = lazy(() =>
  loadCorrectionPanel().then((m) => ({ default: m.CorrectionPanel })),
);
const CorrectionTotal = lazy(() =>
  loadCorrectionPanel().then((m) => ({ default: m.CorrectionTotal })),
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
// "Gói bán hôm nay" (label + one button) while its chunk loads.
const packagesBox = <div aria-hidden="true" className="h-[70px]" />;

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
    named: c.named,
    phone: c.phone ?? null,
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
  const showsCalendar = search.view === "lich" && can(me, "appointment.manage");
  // Today's visits order the technician pickers and fill "Lượt hôm nay", both on "Ghi lượt" only:
  // the calendar tab asks for its own days (perf page §2d, one request less there).
  const today = useQuery({
    ...todayVisitsQuery,
    enabled: painted && !showsCalendar,
  });
  useEffect(() => {
    if (!painted) return;
    void loadTodayVisits();
    void loadCartOptions();
    void loadCartPackages();
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
  /** "Gói bán hôm nay": packages sold in this checkout. */
  const [sales, setSales] = useState<PackageSaleDraft[]>([]);
  /**
   * "Người đi cùng" notes of the booking this visit completes (ADR-0009: companions are entered on
   * the booking; one who is served gets their own booking and visit). Shown, sent, not edited.
   */
  const [companions, setCompanions] = useState<string[]>([]);
  /** The customer card asked for a package block ("" = empty block, else that plan). */
  const [openPlan, setOpenPlan] = useState<string | null>(null);
  /** The cart came back from an unsaved draft: say so, offer "Bỏ giỏ". */
  const [restored, setRestored] = useState(false);
  /** Picked for a fresh start (a booking, a correction, "Bỏ giỏ"): its draft is not restored. */
  const [freshFor, setFreshFor] = useState<string | null>(null);
  /** A customer change waiting for "Đổi khách" (the cart holds items). */
  const [leaving, setLeaving] = useState<(() => void) | null>(null);
  /** The one cart item open for editing ("l<key>" line, "s<ref>" package); the rest fold. */
  const [openItem, setOpenItem] = useState<string | null>(null);
  /** "Thêm thông tin khách" is open for the selected nameless walk-in. */
  const [editingInfo, setEditingInfo] = useState(false);
  const clearOpenPlan = useCallback(() => {
    setOpenPlan(null);
  }, []);
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

  // Lines a package sold today covers are today's session from it: 0đ (spec §10.4.1).
  const covered = coverage(lines, sales);
  const due = amountDue(lines, covered);
  const discountAmount = discountPreview(due, discount);
  const packagesTotal = packagesDue(sales);
  const payable = due - discountAmount + packagesTotal;
  const planOf = (ref: string | undefined) =>
    sales.find((s) => s.ref === ref)?.plan?.name ?? "gói mua hôm nay";
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
    setSales([]);
    setCompanions([]);
    setOpenPlan(null);
    setRestored(false);
    setOpenItem(null);
    setEditingInfo(false);
    setLeaving(null);
    setIdempotencyKey(crypto.randomUUID());
    setSearchGeneration((g) => g + 1);
    setTimeout(() => searchInput.current?.focus(), 0);
  };

  /** Forget the saved draft of this customer (saved, abandoned). */
  const forgetDraft = (customerId: string | undefined) => {
    if (me && customerId)
      void import("./draftCart").then((m) => {
        m.clearDraft(me.user.id, customerId);
      });
  };
  /** The cart of a stored draft, back on screen. */
  const applyDraft = (d: CartDraft) => {
    setCreating(null);
    setCustomer(d.customer);
    setLines(d.lines.map((l) => ({ ...l, key: nextKey.current++ })));
    setSales(d.sales);
    setMode(d.mode);
    setCashText(d.cashText);
    setDiscount(d.discount);
    setServiceDay(d.serviceDay);
    setServiceTime(d.serviceTime);
    setCompanions(d.companions);
    setAppointment(d.appointment);
    setAttempted(false);
    setIdempotencyKey(crypto.randomUUID());
    setFocusDeduct(false);
    setRestored(true);
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
    onSuccess: (visit: Visit, body) => {
      const sentNote = body.note;
      queryClient.setQueryData<Visit[]>(todayVisitsQuery.queryKey, (old) =>
        visit.business_date === todayIso
          ? [
              // The saved response never carries the note (health data): show the one just typed.
              { ...visit, ...(sentNote ? { note: sentNote } : {}) },
              ...(old ?? []).filter((v) => v.id !== visit.id),
            ]
          : old,
      );
      // Walk-ins are on the calendar too (spec appointments.md §10.1), not only completed bookings.
      void queryClient.invalidateQueries({ queryKey: ["appointments"] });
      if (visit.business_date !== todayIso)
        void queryClient.invalidateQueries({
          queryKey: ["visits", visit.business_date],
        });
      // Not awaited: the toast does not wait a round trip for the balance (perf page §4d). The
      // balance refetches behind it and has a 5 s staleTime, so the next open of this customer asks again.
      void queryClient.invalidateQueries({
        queryKey: ["customer-packages", visit.customer_id],
      });
      void invalidateManagementViews(queryClient, visit.customer_id);
      for (const ps of visit.package_sales)
        if (ps.beneficiary_customer_id)
          void queryClient.invalidateQueries({
            queryKey: ["customer-packages", ps.beneficiary_customer_id],
          });
      forgetDraft(visit.customer_id);
      showToast(
        savedSummary(visit) +
          visit.package_sales
            .map(
              (p) =>
                ` · bán ${p.description}${p.beneficiary_name ? ` cho ${p.beneficiary_name}` : ""}`,
            )
            .join("") +
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
  // A cart with only a package and no service is a plain package sale (spec §10.4.1, US10).
  const packageOnly = useMutation({
    mutationFn: async (sale: PackageSaleDraft) => {
      if (!customer) throw new Error("no customer");
      const m = await loadCartPackages();
      return m.sellPackageOnly(
        customer.id,
        sale,
        paymentsFor(payable, mode, cashPart),
        idempotencyKey,
      );
    },
    onSuccess: (sale, draft) => {
      void queryClient.invalidateQueries({ queryKey: ["customer-packages"] });
      void invalidateManagementViews(queryClient, customer?.id);
      if (draft.beneficiary)
        void invalidateManagementViews(queryClient, draft.beneficiary.id);
      forgetDraft(customer?.id);
      const sold = sale.lines.map((l) => l.description).join(", ");
      showToast(
        draft.beneficiary
          ? boughtForOtherText(
              sold,
              draft.beneficiary.full_name,
              customer?.full_name ?? "",
            )
          : `Đã bán ${sold} · ${customer?.full_name ?? ""}`,
      );
      resetVisit();
    },
    onError: (error) => {
      showToast(messageFor(error), true);
    },
  });
  const saving =
    mutation.isPending || correction.isPending || packageOnly.isPending;
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

  /** `packageSales`: the cart's packages in wire form (built by the lazy cart chunk). */
  const record = (packageSales?: VisitPackageSaleCreate[]) => {
    if (!customer) return;
    mutation.mutate(
      visitBody(
        customer.id,
        lines,
        paymentsFor(payable, mode, cashPart),
        can(me, "customer.health_notes.write") ? note : "",
        {
          discount: due > 0 ? discount : null,
          checkInAt: serviceInstant(serviceDay || todayIso, serviceTime),
          ...(packageSales ? { packageSales, covered } : {}),
          companions,
        },
      ),
    );
  };

  const save = () => {
    if (!customer || (lines.length === 0 && sales.length === 0) || saving)
      return;
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
    if (sales.length > 0) {
      void loadCartPackages().then((m) => {
        const problem = m.checkoutProblem(
          sales,
          lines.length,
          appointment !== null,
          can(me, "sale.discount_unlimited"),
          customer.named,
        );
        if (problem) showToast(problem, true);
        else if (lines.length > 0) record(m.packageSalesBody(sales));
        else if (sales[0]) packageOnly.mutate(sales[0]);
      });
      return;
    }
    record();
  };

  // Keyboard: "/" → search, Ctrl+Enter → save, Esc → drop the selected customer.
  const dropCustomer = () => {
    if (!customer) return;
    setCustomer(null);
    setLines([]);
    setCreating(null);
    setAppointment(null);
    setDiscount(null);
    setSales([]);
    setCompanions([]);
    setRestored(false);
    setOpenItem(null);
    setEditingInfo(false);
    searchInput.current?.focus();
  };
  /** Runs `go` (a customer change), first asking when it would empty a cart with items. */
  const leaveCart = (go: () => void) => {
    if (customer && (lines.length > 0 || sales.length > 0))
      setLeaving(() => go);
    else go();
  };
  const saveRef = useRef(save);
  const dropRef = useRef(dropCustomer);
  useEffect(() => {
    saveRef.current = save;
    dropRef.current = () => {
      if (leaving) setLeaving(null);
      else leaveCart(dropCustomer);
    };
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

  /** `fresh`: a booking, a correction or "Bỏ giỏ" starts clean (no draft comes back). */
  const select = (c: CustomerSummary, fresh = false) => {
    setCreating(null);
    setCustomer(c);
    setLines([]);
    setAttempted(false);
    setDiscount(null);
    setAppointment(null);
    setSales([]);
    setCompanions([]);
    setOpenPlan(null);
    setRestored(false);
    setOpenItem(null);
    setEditingInfo(false);
    setFreshFor(fresh ? c.id : null);
    setLeaving(null);
    setIdempotencyKey(crypto.randomUUID());
    setFocusDeduct(true);
  };
  // What DraftKeeper stores (not while correcting a recorded visit; never the session note).
  const draft: CartDraft | null =
    customer && !correcting
      ? {
          v: 1,
          customer,
          lines,
          sales,
          mode,
          cashText,
          discount,
          serviceDay,
          serviceTime,
          companions,
          appointment,
        }
      : null;

  const addLine = (line: CartLine) => {
    setLines((ls) => [...ls, line]);
    setOpenItem(`l${String(line.key)}`);
    focusLineKey.current = line.key;
  };

  /** Calendar → "Khách đến · ghi lượt": the cart filled with the booked services and technician. */
  const startFromAppointment = (a: Appointment, prefilled: CartLine[]) => {
    if (!a.customer_id) return;
    select(
      {
        id: a.customer_id,
        full_name: a.customer_name,
        // A booking always carries the person's name (ADR-0009).
        named: true,
        phone: a.customer_phone ? localPhone(a.customer_phone) : null,
        gender: a.customer_gender ?? null,
      },
      true,
    );
    setFocusDeduct(false);
    setAppointment(a);
    setCompanions(a.companions);
    setLines(prefilled.map((l) => ({ ...l, key: nextKey.current++ })));
    setSearch({ view: "ghi" }, { push: true });
    if (a.business_date !== todayIso) {
      setServiceDay(a.business_date);
      setServiceTime(timeOf(a.arrived_at ?? a.starts_at));
    }
  };

  /** "Sửa lượt": the cart filled with the recorded visit; the desk changes only what is wrong. */
  const startCorrection = (c: CorrectionStart) => {
    select(c.customer, true);
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

      {showsCalendar ? (
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
                asHeading
                aside={shortcuts}
                onSelect={(c) => {
                  leaveCart(() => {
                    select(c);
                  });
                }}
                onCreate={(typed) => {
                  setCreating(typed);
                }}
                {...(can(me, "customer.write")
                  ? {
                      onWalkIn: (c: Customer) => {
                        leaveCart(() => {
                          select(summaryOf(c));
                        });
                      },
                    }
                  : {})}
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
              {leaving && customer && (
                <Suspense fallback={null}>
                  <LeaveCartConfirm
                    customerName={customer.full_name}
                    correcting={correcting !== null}
                    onLeave={() => {
                      setLeaving(null);
                      leaving();
                    }}
                    onStay={() => {
                      setLeaving(null);
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
                  onClear={() => {
                    leaveCart(dropCustomer);
                  }}
                  {...(can(me, "customer.write")
                    ? {
                        onEditInfo: () => {
                          setEditingInfo(true);
                        },
                      }
                    : {})}
                  {...(can(me, "package.sell") && !correcting
                    ? {
                        onSellPackage: (planId: string | null) => {
                          setOpenPlan(planId ?? "");
                          setTimeout(() => {
                            const el = document.getElementById(
                              "cart-packages-title",
                            );
                            // jsdom has no scrollIntoView.
                            if (el && "scrollIntoView" in el)
                              el.scrollIntoView({ block: "center" });
                          }, 0);
                        },
                      }
                    : {})}
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
                <p className="empty-note">
                  Chưa chọn khách. Gõ SĐT hoặc tên ở ô phía trên (không cần
                  dấu); khách đặt trước thì mở tab “Lịch hẹn”.
                </p>
              )}
              {editingInfo && customer && (
                <Suspense fallback={null}>
                  <WalkInInfo
                    customerId={customer.id}
                    canWriteHealth={can(me, "customer.health_notes.write")}
                    onSaved={(c) => {
                      setEditingInfo(false);
                      setCustomer(summaryOf(c));
                      queryClient.setQueryData(customerQuery(c.id).queryKey, c);
                    }}
                    onCancel={() => {
                      setEditingInfo(false);
                    }}
                  />
                </Suspense>
              )}
            </section>

            <section aria-labelledby="cart-title" className="panel">
              <h2 id="cart-title" className="panel-title scroll-mt-20">
                {correcting ? "Sửa lượt khách" : "Lượt khách đang ghi"}
              </h2>
              {(correcting || appointment || (restored && customer)) && (
                <Suspense fallback={null}>
                  <CartContextBars
                    correcting={correcting?.visit ?? null}
                    onStopCorrecting={resetVisit}
                    appointment={appointment}
                    today={todayIso}
                    restoredFor={
                      restored && !correcting
                        ? (customer?.full_name ?? null)
                        : null
                    }
                    onUnlink={() => {
                      setAppointment(null);
                      setServiceDay("");
                      setServiceTime("");
                    }}
                    onDiscard={() => {
                      if (!customer) return;
                      forgetDraft(customer.id);
                      select(customer, true);
                      setFocusDeduct(false);
                    }}
                  />
                </Suspense>
              )}
              {!customer && (
                <p className="empty-note">Chọn khách để bắt đầu ghi lượt.</p>
              )}
              {customer && lines.length === 0 && sales.length === 0 && (
                <p className="empty-note">
                  Chưa có dịch vụ. Bấm “Trừ 1 buổi” ở gói liệu trình, chọn dịch
                  vụ lẻ bên dưới, hoặc bán gói liệu trình.
                </p>
              )}
              {lines.length > 0 && (
                <Suspense fallback={null}>
                  <CartLines
                    lines={lines}
                    onLines={setLines}
                    covered={covered}
                    planName={planOf}
                    attempted={attempted}
                    staff={staff.data ?? []}
                    staffPending={staff.isPending}
                    technicians={technicians}
                    staffSelects={staffSelects}
                    focusLineKey={focusLineKey}
                    openKey={openItem}
                    onOpen={setOpenItem}
                  />
                </Suspense>
              )}

              {customer && (
                <Suspense fallback={addOneOffBox}>
                  <AddOneOff
                    key={customer.id}
                    customer={customer}
                    lines={lines}
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

              {customer && !correcting && can(me, "package.sell") && (
                <Suspense fallback={packagesBox}>
                  <PackageSales
                    sales={sales}
                    onSales={setSales}
                    lines={lines}
                    covered={covered}
                    staff={staff.data ?? []}
                    customerId={customer.id}
                    canDiscount={
                      can(me, "sale.discount") ||
                      can(me, "sale.discount_unlimited")
                    }
                    unlimitedDiscount={can(me, "sale.discount_unlimited")}
                    attempted={attempted}
                    serverError={mutation.error}
                    openPlanId={openPlan}
                    onOpened={clearOpenPlan}
                    ownerNamed={customer.named}
                    openRef={openItem}
                    onOpenRef={setOpenItem}
                    onLines={setLines}
                    onTodaySession={(saleRef, s) => {
                      addLine({
                        key: nextKey.current++,
                        source: "new_package",
                        saleRef,
                        ...s,
                        staffId: appointment?.staff_id ?? "",
                      });
                    }}
                  />
                </Suspense>
              )}
              {companions.length > 0 && (
                <p className="field-hint">
                  Người đi cùng (ghi chú của lịch hẹn): {companions.join(", ")}
                </p>
              )}

              {customer && (lines.length > 0 || sales.length > 0) && (
                <>
                  <div className="cart-options">
                    <Suspense fallback={null}>
                      {lines.length > 0 && (
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
                      )}
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
                    <Suspense fallback={null}>
                      <PaymentChoices
                        name="visit-payment"
                        mode={mode}
                        onMode={setMode}
                        cashText={cashText}
                        onCashText={setCashText}
                        total={payable}
                        invalid={attempted && problems.cash}
                      />
                    </Suspense>
                  )}
                  {can(me, "customer.health_notes.write") && lines.length > 0 && (
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
                    {correcting ? (
                      <Suspense fallback={null}>
                        <CorrectionTotal
                          total={payable}
                          paid={correcting.visit.total}
                        />
                      </Suspense>
                    ) : (
                      <p className="checkout-total" aria-live="polite">
                        <span>Phải trả</span>
                        <span className="checkout-amount">
                          {discountAmount > 0 && <s>{formatVnd(due)}</s>}
                          {formatVnd(payable)}
                        </span>
                      </p>
                    )}
                    {packagesTotal > 0 && (
                      <p className="checkout-split">
                        Dịch vụ lẻ {formatVnd(due - discountAmount)} + gói{" "}
                        {formatVnd(packagesTotal)}
                      </p>
                    )}
                    {payable === 0 && sales.length === 0 && (
                      <p className="checkout-split">
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
                          : lines.length === 0
                            ? "Lưu hóa đơn gói"
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
                resetSignal={searchGeneration}
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

      {painted && me && (
        <Suspense fallback={null}>
          <DraftKeeper
            userId={me.user.id}
            draft={draft}
            customerId={customer?.id ?? null}
            fresh={freshFor === customer?.id || search.sua !== ""}
            onRestore={applyDraft}
          />
        </Suspense>
      )}
      <div
        role="status"
        aria-live="polite"
        className={`toast ${toast ? "is-shown" : ""} ${toast?.error ? "is-error" : ""}`}
      >
        {toast?.text}
      </div>
    </div>
  );
}
