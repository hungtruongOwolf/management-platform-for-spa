import type {
  Appointment,
  Customer,
  CustomerSummary,
  Visit,
  VisitCreate,
} from "@vpyd/api-client";
import {
  useMutation,
  useQuery,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query";
import {
  Suspense,
  lazy,
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";
import { useAfterFirstPaint } from "../../lib/afterFirstPaint";
import { useSearchState } from "../../lib/searchState";
import { messageFor } from "../../lib/errors";
import { can, meQuery } from "../auth/session";
import { customerQuery } from "../customers/api";
import { CustomerSearch } from "../customers/CustomerSearch";
import { activeStaffQuery } from "../sales/api";
import type {
  CartLine,
  DiscountDraft,
  PackageSaleDraft,
  PaymentMode,
} from "./api";
import type { CorrectionToSave, DeskSaves } from "./CheckoutPanel";
import { inCart, todayVisitsQuery } from "./desk";
import type { CartDraft } from "./draftCart";
import { Tip } from "./Tip";
import "./front-desk.css";
import type { CorrectionSaved, CorrectionStart } from "./correction";
// The countdown ticks from just after the first paint (own chunk; its box is reserved meanwhile).
const DayCountdown = lazy(() =>
  import("./DayCountdown").then((m) => ({ default: m.DayCountdown })),
);
import { frontDeskSearch } from "./search";
import { spaTime, spaToday } from "../../lib/format";

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
// "Ai ghi, ai sửa" of an opened visit (ADR-0022), for those who may see it: the lazy chunk of
// features/record-history/lazy.tsx (same specifier, same chunk).
const loadRecordHistory = () => import("../record-history/chunk");
// The picked customer's card (packages, Trừ 1 buổi) shows only after a search: its own chunk,
// prefetched after the first paint like the cart's (startup JS budget, 2026-11-08).
const loadCustomerCard = () => import("./CustomerCard");
const CustomerCard = lazy(() =>
  loadCustomerCard().then((m) => ({ default: m.CustomerCard })),
);
const loadCartOptions = () => import("./CartOptions");
const TodayVisits = lazy(() =>
  loadTodayVisits().then((m) => ({ default: m.TodayVisits })),
);
const LeaveCartConfirm = lazy(() =>
  loadCartOptions().then((m) => ({ default: m.LeaveCartConfirm })),
);
const WalkInInfo = lazy(() =>
  loadCartOptions().then((m) => ({ default: m.WalkInInfo })),
);
// Round 6: packages sold in the checkout (spec §10.4.1), and the unsaved-cart
// draft (Q-U8). Prefetched after the first paint, never on the startup path.
const loadCartPackages = () => import("./CartPackages");
const DraftKeeper = lazy(() =>
  loadCartPackages().then((m) => ({ default: m.DraftKeeper })),
);
// The cart under its title (lines, payment, Lưu) and what each save does: its own chunk, mounted
// after the first paint like the cart's other parts (/quay critical path, frontend.md).
const loadCheckout = () => import("./CheckoutPanel");
const CheckoutPanel = lazy(() =>
  loadCheckout().then((m) => ({ default: m.CheckoutPanel })),
);
// "Sửa lượt" (US6b): only when a recorded visit is opened for correction.
const loadCorrection = () => import("./correction");

// Placeholders for the lazy pieces until they are mounted after the first paint: the same boxes,
// so swapping them in shifts nothing (CLS budget).
const countdownBox = <span aria-hidden="true" className="day-clock" />;
const todayVisitsSkeleton = (
  <section aria-label="Lượt khách hôm nay" className="panel">
    <div aria-hidden="true" className="h-24 skeleton" />
  </section>
);
// The empty cart of the first paint, drawn here until the checkout chunk lands.
const emptyCart = (
  <p className="empty-note">Chọn khách để bắt đầu ghi lượt.</p>
);

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
  const traces = can(me, "record.history.read");
  useEffect(() => {
    if (!painted) return;
    void loadCustomerCard();
    void loadTodayVisits();
    void loadCartOptions();
    void loadCartPackages();
    void loadCheckout();
    if (traces) void loadRecordHistory();
  }, [painted, traces]);
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

  // The three saves keep their state here, so a save in flight outlives the calendar tab; what each
  // one sends and does afterwards comes from the checkout chunk, where every save starts.
  const saves = useRef<DeskSaves | null>(null);
  const on = () => {
    if (!saves.current) throw new Error("checkout not loaded");
    return saves.current;
  };
  const mutation = useMutation({
    mutationFn: (body: VisitCreate) => on().recordVisit(body),
    onSuccess: (visit: Visit, body) => {
      on().visitSaved(visit, body);
    },
    onError: (error) => on().visitFailed(error),
  });
  const correction = useMutation({
    mutationFn: (plan: CorrectionToSave) => on().saveCorrection(plan),
    onSuccess: (saved: CorrectionSaved) => {
      on().correctionSaved(saved);
    },
    onError: (error) => {
      on().correctionFailed(error);
    },
  });
  const packageOnly = useMutation({
    mutationFn: (drafts: PackageSaleDraft[]) => on().sellPackages(drafts),
    onSuccess: (soldText: string, drafts) => {
      on().packagesSold(soldText, drafts);
    },
    onError: (error) => {
      on().packagesFailed(error);
    },
  });
  /** Sessions the visit being corrected holds: given back before the correction re-uses them. */
  const heldByCorrected = (itemId: string) =>
    correcting && customer?.id === correcting.visit.customer_id
      ? correcting.visit.lines
          .filter((l) => l.customer_package_item_id === itemId)
          .reduce((n, l) => n + l.qty, 0)
      : 0;

  /** A save that failed for the previous customer says nothing about the next one. */
  const forgetSaveErrors = () => {
    mutation.reset();
    correction.reset();
    packageOnly.reset();
  };
  // Keyboard: "/" → search, Ctrl+Enter → save, Esc → drop the selected customer.
  const dropCustomer = () => {
    if (!customer) return;
    forgetSaveErrors();
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
  const dropRef = useRef(dropCustomer);
  useEffect(() => {
    dropRef.current = () => {
      if (leaving) setLeaving(null);
      else leaveCart(dropCustomer);
    };
  });
  useEffect(() => {
    // The cart is hidden behind the calendar tab; a sheet or palette keeps its own keys.
    if (showsCalendar) return;
    const onKey = (e: KeyboardEvent) => {
      if (
        e.defaultPrevented ||
        (e.target instanceof Element && e.target.closest("dialog[open]"))
      )
        return;
      const typing = isTyping(document.activeElement);
      if (e.key === "/" && !typing) {
        e.preventDefault();
        searchInput.current?.focus();
      } else if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
        e.preventDefault();
        // The checkout's "Lưu"; before its chunk lands no customer is picked: nothing to save.
        saves.current?.save();
      } else if (e.key === "Escape" && !typing) {
        dropRef.current();
      }
    };
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("keydown", onKey);
    };
  }, [showsCalendar]);

  /** `fresh`: a booking, a correction or "Bỏ giỏ" starts clean (no draft comes back). */
  const select = (c: CustomerSummary, fresh = false) => {
    forgetSaveErrors();
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
      setServiceTime(spaTime(a.arrived_at ?? a.starts_at));
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
                <Suspense
                  fallback={
                    <div aria-hidden="true" className="h-40 skeleton" />
                  }
                >
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
                        ...(expiryOverrideReason
                          ? { expiryOverrideReason }
                          : {}),
                      });
                    }}
                  />
                </Suspense>
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
              {painted ? (
                <Suspense
                  fallback={
                    customer ? (
                      <div aria-hidden="true" className="h-40 skeleton" />
                    ) : (
                      emptyCart
                    )
                  }
                >
                  <CheckoutPanel
                    me={me}
                    customer={customer}
                    lines={lines}
                    onLines={setLines}
                    sales={sales}
                    onSales={setSales}
                    mode={mode}
                    onMode={setMode}
                    cashText={cashText}
                    onCashText={setCashText}
                    note={note}
                    onNote={setNote}
                    discount={discount}
                    onDiscount={setDiscount}
                    serviceDay={serviceDay}
                    onServiceDay={setServiceDay}
                    serviceTime={serviceTime}
                    onServiceTime={setServiceTime}
                    correcting={correcting}
                    reason={reason}
                    onReason={setReason}
                    appointment={appointment}
                    onAppointment={setAppointment}
                    companions={companions}
                    restored={restored}
                    attempted={attempted}
                    onAttempted={setAttempted}
                    openItem={openItem}
                    onOpenItem={setOpenItem}
                    openPlan={openPlan}
                    onOpenedPlan={clearOpenPlan}
                    onFocusDeduct={setFocusDeduct}
                    staff={staff.data ?? []}
                    staffPending={staff.isPending}
                    today={today.data ?? []}
                    idempotencyKey={idempotencyKey}
                    todayIso={todayIso}
                    staffSelectsRef={staffSelects}
                    focusLineKeyRef={focusLineKey}
                    nextKeyRef={nextKey}
                    addLine={addLine}
                    resetVisit={resetVisit}
                    select={select}
                    forgetDraft={forgetDraft}
                    showToast={showToast}
                    visit={mutation}
                    correction={correction}
                    packageOnly={packageOnly}
                    savesRef={saves}
                  />
                </Suspense>
              ) : (
                emptyCart
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
