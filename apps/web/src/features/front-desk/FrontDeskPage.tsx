import type {
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
import { ApiError } from "../../lib/api";
import { messageFor } from "../../lib/errors";
import { can, meQuery } from "../auth/session";
import { categoriesQuery, servicesQuery } from "../catalog/api";
import { customerQuery } from "../customers/api";
import { CustomerSearch } from "../customers/CustomerSearch";
import { activeStaffQuery, parseAmount } from "../sales/api";
import {
  type CartLine,
  type Gender,
  type PaymentMode,
  amountDue,
  inCart,
  paymentsFor,
  priceFor,
  recordVisit,
  savedSummary,
  staffByUse,
  timeOf,
  todayVisitsQuery,
  visitBody,
  visitsOfDayQuery,
} from "./api";
import { BrandMark } from "../../app/BrandMark";
import { CustomerCard } from "./CustomerCard";
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

/**
 * /quay — the front desk. Fast path (design/front-desk-flows.md): find the customer + Enter →
 * "Trừ 1 buổi" → pick the technician → Lưu (Ctrl+Enter). Every amount shown is a preview; the
 * server prices the visit and enforces every rule.
 */
export function FrontDeskPage() {
  const { data: me } = useSuspenseQuery(meQuery);
  const queryClient = useQueryClient();
  const staff = useQuery(activeStaffQuery);
  const today = useQuery(todayVisitsQuery);

  const [customer, setCustomer] = useState<CustomerSummary | null>(null);
  const [creating, setCreating] = useState<string | null>(null);
  const [lines, setLines] = useState<CartLine[]>([]);
  const [attempted, setAttempted] = useState(false);
  const [mode, setMode] = useState<PaymentMode>("cash");
  const [cashText, setCashText] = useState("");
  const [note, setNote] = useState("");
  const [toast, setToast] = useState<Toast | null>(null);
  const [focusDeduct, setFocusDeduct] = useState(false);
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
  const cashPart = parseAmount(cashText);
  const technicians = staffByUse(staff.data ?? [], today.data ?? []);

  const resetVisit = () => {
    setCustomer(null);
    setLines([]);
    setAttempted(false);
    setMode("cash");
    setCashText("");
    setNote("");
    setIdempotencyKey(crypto.randomUUID());
    setSearchGeneration((g) => g + 1);
    setTimeout(() => searchInput.current?.focus(), 0);
  };

  const mutation = useMutation({
    mutationFn: (body: Parameters<typeof recordVisit>[0]) =>
      recordVisit(body, idempotencyKey),
    onSuccess: async (visit: Visit) => {
      queryClient.setQueryData<Visit[]>(todayVisitsQuery.queryKey, (old) => [
        visit,
        ...(old ?? []).filter((v) => v.id !== visit.id),
      ]);
      await queryClient.invalidateQueries({
        queryKey: ["customer-packages", visit.customer_id],
      });
      showToast(savedSummary(visit));
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

  const problems = {
    staff: lines.some((l) => l.staffId === ""),
    cash:
      due > 0 &&
      mode === "mixed" &&
      (Number.isNaN(cashPart) || cashPart <= 0 || cashPart >= due),
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
    mutation.mutate(
      visitBody(
        customer.id,
        lines,
        paymentsFor(due, mode, cashPart),
        can(me, "customer.health_notes.write") ? note : "",
      ),
    );
  };

  // Keyboard: "/" → search, Ctrl+Enter → save, Esc → drop the selected customer.
  const dropCustomer = () => {
    if (!customer) return;
    setCustomer(null);
    setLines([]);
    setCreating(null);
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
    setIdempotencyKey(crypto.randomUUID());
    setFocusDeduct(true);
  };

  const addLine = (line: CartLine) => {
    setLines((ls) => [...ls, line]);
    focusLineKey.current = line.key;
  };

  return (
    <div className="grid gap-4 lg:grid-cols-[minmax(0,1.1fr)_minmax(0,1fr)]">
      <h1 className="sr-only">Màn hình quầy</h1>
      <div className="grid min-w-0 content-start gap-4">
        <section aria-label="Tìm khách" className={card}>
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
            <kbd>↑</kbd> <kbd>↓</kbd> chọn, <kbd>Enter</kbd> mở, <kbd>/</kbd> về
            ô tìm, <kbd>Ctrl</kbd>+<kbd>Enter</kbd> lưu, <kbd>Esc</kbd> bỏ
            khách.
          </p>
        </section>
        {customer ? (
          <CustomerCard
            key={customer.id}
            summary={customer}
            inCart={(itemId) => inCart(lines, itemId)}
            canOverrideExpiry={can(me, "package.use_expired_override")}
            focusFirst={focusDeduct}
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
                itemId: item.id,
                packageName: pkg.name,
                staffId: "",
                ...(expiryOverrideReason ? { expiryOverrideReason } : {}),
              });
            }}
          />
        ) : (
          <p className={`${card} text-center text-text-muted`}>
            <BrandMark className="mx-auto mb-2 block size-10 text-accent opacity-70" />
            Chưa chọn khách. Tìm khách ở ô phía trên để bắt đầu một lượt.
          </p>
        )}
      </div>

      <div className="grid min-w-0 content-start gap-4">
        <section aria-labelledby="cart-title" className={card}>
          <h2 id="cart-title" className="mb-3 font-semibold">
            Lượt khách đang ghi
          </h2>
          {!customer && (
            <p className="text-text-muted">Chọn khách để bắt đầu ghi lượt.</p>
          )}
          {customer && lines.length === 0 && (
            <p className="text-text-muted">
              Chưa có dịch vụ. Bấm “Trừ 1 buổi” ở liệu trình hoặc thêm dịch vụ
              lẻ.
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
                      <strong>{l.serviceName}</strong>
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
                          setLines((ls) => ls.filter((x) => x.key !== l.key));
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
              onAdd={(service, priceVariant, price) => {
                addLine({
                  key: nextKey.current++,
                  source: "one_off",
                  serviceId: service.id,
                  serviceName: service.name,
                  priceVariant,
                  price,
                  staffId: "",
                });
              }}
              onProblem={(m) => {
                showToast(m, true);
              }}
            />
          )}

          {customer && lines.length > 0 && (
            <div className="mt-4 grid content-start gap-3">
              <p
                className="flex justify-between text-lg font-bold"
                aria-live="polite"
              >
                <span>Phải trả</span>
                <span className="tabular-nums">{formatVnd(due)}</span>
              </p>
              {due > 0 ? (
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
                        cashPart < due && (
                          <span className="block text-xs text-text-muted">
                            Chuyển khoản: {formatVnd(due - cashPart)}
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
              {mutation.isError && !(mutation.error instanceof ApiError) && (
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

        <TodayVisits />
      </div>

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
  onAdd: (service: Service, priceVariant: Gender | null, price: number) => void;
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

/** A busy day holds a hundred visits; the latest come first, the rest on request. Keeps the screen
 * that stays open all day light to render (Lighthouse LCP/TBT budget, testing.md §4). */
const VISITS_PAGE = 30;

function TodayVisits() {
  const { data: me } = useSuspenseQuery(meQuery);
  const queryClient = useQueryClient();
  const todayIso = spaToday();
  const [day, setDay] = useState(todayIso);
  const [voiding, setVoiding] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [shown, setShown] = useState(VISITS_PAGE);
  const isToday = day === todayIso;
  const today = useQuery(isToday ? todayVisitsQuery : visitsOfDayQuery(day));
  // UX only: the server decides (403 day.locked for managers; owners may).
  const mayVoid =
    can(me, "record.void_same_day") &&
    (isToday || can(me, "record.void_closed_day"));
  return (
    <section aria-labelledby="today-title" className={card}>
      <div className="mb-2 flex flex-wrap items-center justify-between gap-2">
        <h2 id="today-title" className="font-semibold">
          {isToday
            ? "Lượt khách hôm nay"
            : `Lượt khách ngày ${day.split("-").reverse().join("/")}`}
          {today.data ? ` (${String(today.data.length)})` : ""}
        </h2>
        <label className="flex items-center gap-2 text-sm">
          Xem ngày
          <input
            type="date"
            value={day}
            max={todayIso}
            onChange={(e) => {
              setDay(e.target.value || todayIso);
              setVoiding(null);
              setShown(VISITS_PAGE);
            }}
            className="min-h-11 rounded-[6px] border border-border bg-surface px-2"
          />
        </label>
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
          {today.data.slice(0, shown).map((v) => (
            <li
              key={v.id}
              className="border-b border-dashed border-border py-2 text-sm last:border-b-0"
            >
              <div className="flex justify-between gap-2">
                <span
                  className={`min-w-0 ${v.status === "voided" ? "text-text-muted" : ""}`}
                >
                  <strong>{timeOf(v.check_in_at)}</strong> · {v.customer_name}
                  {v.status === "voided" && (
                    <span className="badge badge-danger ml-2 align-middle">
                      Đã hủy
                    </span>
                  )}
                  <span className="block text-text-muted">
                    {v.lines
                      .map(
                        (l) =>
                          `${l.service_name}${l.source === "package" ? " (trừ buổi)" : ""}`,
                      )
                      .join(", ")}{" "}
                    · KTV{" "}
                    {[...new Set(v.lines.map((l) => l.staff_name))].join(", ")}
                  </span>
                  {v.status === "voided" && v.void_reason && (
                    <span className="block">Lý do hủy: {v.void_reason}</span>
                  )}
                </span>
                <span className="flex shrink-0 items-start gap-2">
                  <span
                    className={`tabular-nums ${v.status === "voided" ? "text-text-muted line-through" : ""}`}
                  >
                    {v.total ? formatVnd(v.total) : "—"}
                  </span>
                  {mayVoid && v.status === "completed" && voiding !== v.id && (
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
                      }}
                    />
                  </div>
                </Suspense>
              )}
            </li>
          ))}
        </ul>
      )}
      {today.data && today.data.length > shown && (
        <button
          type="button"
          onClick={() => {
            setShown((n) => n + VISITS_PAGE);
          }}
          className="mt-2 min-h-11 w-full rounded-[10px] border border-border px-4 font-medium hover:bg-surface-2"
        >
          Xem thêm ({today.data.length - shown} lượt nữa)
        </button>
      )}
    </section>
  );
}
