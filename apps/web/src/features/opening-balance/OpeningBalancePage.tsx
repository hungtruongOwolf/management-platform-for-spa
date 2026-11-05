import type { CustomerSummary, OpeningBalance } from "@vpyd/api-client";
import {
  useMutation,
  useQuery,
  useQueryClient,
  useSuspenseQuery,
} from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { formatVnd } from "@vpyd/ui";
import { useState } from "react";
import { ApiError } from "../../lib/api";
import { fieldMessage, messageFor } from "../../lib/errors";
import { can, meQuery } from "../auth/session";
import { plansQuery, servicesQuery } from "../catalog/api";
import { prefillFrom } from "../customers/CustomerForm";
import { CustomerSearch } from "../customers/CustomerSearch";
import { parseAmount } from "../sales/api";
import {
  type ItemDraft,
  type PackageDraft,
  emptyItem,
  emptyPackage,
  fromPlan,
  importOpeningBalance,
  isValid,
  packageProblems,
  toRequest,
  todayInVietnam,
} from "./api";

const input =
  "min-h-11 w-full rounded-[6px] border border-border bg-surface px-3";
const small =
  "min-h-11 w-20 rounded-[6px] border border-border bg-surface px-2";

type Who =
  | { kind: "none" }
  | { kind: "existing"; id: string; label: string }
  | {
      kind: "new";
      fullName: string;
      phone: string;
      gender: "" | "female" | "male";
    };

const Err = ({ children }: { children: string | undefined }) =>
  children ? (
    <span className="block text-sm text-danger">{children}</span>
  ) : null;

/**
 * US8: carry part-used packages over from the paper books, one customer at a time.
 * Nothing here is revenue: the server creates packages only, never a sale or payment.
 */
export function OpeningBalancePage() {
  const { data: me } = useSuspenseQuery(meQuery);
  // UX only: the server answers 403 to anyone without the permission. Owners only while the import
  // waits for its operator CLI (EM 2026-11-05); `package.cancel_refund` is the owner's mark.
  if (!can(me, "opening_balance.import") || !can(me, "package.cancel_refund")) {
    return (
      <p role="alert" className="text-danger">
        Bạn không có quyền nhập liệu trình từ sổ cũ.
      </p>
    );
  }
  return <OpeningBalanceForm />;
}

function OpeningBalanceForm() {
  const queryClient = useQueryClient();
  const plans = useQuery(plansQuery(true));
  const services = useQuery(servicesQuery(true));
  const [who, setWho] = useState<Who>({ kind: "none" });
  const [packages, setPackages] = useState<PackageDraft[]>([emptyPackage()]);
  const [touched, setTouched] = useState(false);
  // One key per customer being entered: a double click or a retry replays instead of importing twice.
  const [idempotencyKey, setIdempotencyKey] = useState(() =>
    crypto.randomUUID(),
  );
  const [round, setRound] = useState(0);
  const [imported, setImported] = useState(0);
  const [last, setLast] = useState<{
    result: OpeningBalance;
    name: string;
  } | null>(null);

  const today = todayInVietnam();
  const problems = packages.map((p) => packageProblems(p, today));
  const customerProblem =
    who.kind === "none"
      ? "Chọn khách hoặc tạo khách mới."
      : who.kind === "new" &&
          (who.fullName.trim() === "" || who.phone.trim() === "")
        ? "Nhập họ tên và số điện thoại."
        : undefined;
  const valid = customerProblem === undefined && problems.every(isValid);
  const serviceName = (id: string) =>
    services.data?.find((s) => s.id === id)?.name;

  const mutation = useMutation({
    mutationFn: () =>
      importOpeningBalance(
        {
          customer_id: who.kind === "existing" ? who.id : null,
          customer:
            who.kind === "new"
              ? {
                  full_name: who.fullName,
                  phone: who.phone,
                  gender: who.gender === "" ? null : who.gender,
                }
              : null,
          packages: packages.map(toRequest),
        },
        idempotencyKey,
      ),
    onSuccess: async (result) => {
      await queryClient.invalidateQueries({
        queryKey: ["customer-packages", result.customer_id],
      });
      await queryClient.invalidateQueries({ queryKey: ["customers"] });
      setLast({
        result,
        name:
          who.kind === "new"
            ? who.fullName.trim()
            : who.kind === "existing"
              ? who.label
              : "",
      });
      setImported((n) => n + result.customer_packages.length);
      // "Lưu và nhập tiếp": a clean form for the next customer, focus back on the search box.
      setWho({ kind: "none" });
      setPackages([emptyPackage()]);
      setTouched(false);
      setIdempotencyKey(crypto.randomUUID());
      setRound((r) => r + 1);
    },
  });
  const existingId =
    mutation.error instanceof ApiError &&
    mutation.error.code === "customer.phone_taken"
      ? mutation.error.existingId
      : undefined;

  const change = (j: number, next: PackageDraft) => {
    setPackages((all) => all.map((p, k) => (k === j ? next : p)));
  };
  const changeItem = (j: number, i: number, patch: Partial<ItemDraft>) => {
    const p = packages[j];
    if (p)
      change(j, {
        ...p,
        items: p.items.map((it, k) => (k === i ? { ...it, ...patch } : it)),
      });
  };
  const show = (message: string | undefined) => (touched ? message : undefined);
  const server = (field: string) => fieldMessage(mutation.error, field);

  return (
    <div className="space-y-4">
      <div>
        <h1 className="text-lg font-semibold">Nhập liệu trình từ sổ cũ</h1>
        <p className="text-sm text-text-muted">
          Chép các liệu trình khách đang dùng dở từ sổ giấy.{" "}
          <strong>Không tính vào doanh thu hôm nay.</strong>
        </p>
        <p role="status" className="text-sm">
          Đã nhập {String(imported)} liệu trình trong lượt làm việc này.
        </p>
      </div>

      {last && (
        <p className="rounded-[10px] bg-primary-soft p-3 text-sm">
          Đã lưu {String(last.result.customer_packages.length)} liệu trình cho{" "}
          {last.name || "khách"}
          {last.result.customer_packages
            .map(
              (p) =>
                ` — ${p.name}: còn ${String(p.sessions_remaining)}/${String(p.sessions_total)} buổi`,
            )
            .join("")}
          .{" "}
          <Link
            to="/khach-hang/$customerId"
            params={{ customerId: last.result.customer_id }}
            className="font-semibold underline"
          >
            Xem hồ sơ
          </Link>
        </p>
      )}

      <section aria-labelledby="ob-customer" className="space-y-2">
        <h2 id="ob-customer" className="font-semibold">
          1. Khách
        </h2>
        {who.kind === "none" && (
          <CustomerSearch
            key={round}
            autoFocus
            onSelect={(c: CustomerSummary) => {
              setWho({
                kind: "existing",
                id: c.id,
                label: `${c.full_name} · ${c.phone}`,
              });
            }}
            onCreate={(typed) => {
              const pre = prefillFrom(typed);
              setWho({
                kind: "new",
                fullName: pre.full_name,
                phone: pre.phone,
                gender: "",
              });
            }}
          />
        )}
        {who.kind === "existing" && (
          <p className="flex flex-wrap items-center gap-2">
            <span className="rounded-[10px] bg-surface-2 px-3 py-2 font-semibold">
              {who.label}
            </span>
            <button
              type="button"
              className="min-h-11 rounded-[10px] border border-border px-3 text-sm"
              onClick={() => {
                setWho({ kind: "none" });
              }}
            >
              Đổi khách
            </button>
          </p>
        )}
        {touched && who.kind === "none" && <Err>{customerProblem}</Err>}
      </section>

      <form
        noValidate
        aria-label="Nhập liệu trình từ sổ cũ"
        onSubmit={(e) => {
          e.preventDefault();
          setTouched(true);
          if (valid && !mutation.isPending) mutation.mutate();
        }}
        className="space-y-4"
      >
        {who.kind === "new" && (
          <fieldset className="grid gap-3 rounded-[10px] border border-dashed border-border p-3 sm:grid-cols-3">
            <legend className="px-1 text-sm font-semibold">Khách mới</legend>
            <label className="block">
              <span className="text-sm font-semibold">Họ tên</span>
              <input
                className={input}
                value={who.fullName}
                onChange={(e) => {
                  setWho({ ...who, fullName: e.target.value });
                }}
              />
              <Err>{server("customer.full_name")}</Err>
            </label>
            <label className="block">
              <span className="text-sm font-semibold">Số điện thoại</span>
              <input
                className={input}
                type="tel"
                inputMode="tel"
                value={who.phone}
                onChange={(e) => {
                  setWho({ ...who, phone: e.target.value });
                }}
              />
              <Err>{server("customer.phone")}</Err>
            </label>
            <label className="block">
              <span className="text-sm font-semibold">Giới tính</span>
              <select
                className={input}
                value={who.gender}
                onChange={(e) => {
                  setWho({
                    ...who,
                    gender: e.target.value as "" | "female" | "male",
                  });
                }}
              >
                <option value="">Chưa rõ</option>
                <option value="female">Nữ</option>
                <option value="male">Nam</option>
              </select>
            </label>
            <Err>{show(customerProblem)}</Err>
            <button
              type="button"
              className="min-h-11 justify-self-start rounded-[10px] border border-border px-3 text-sm sm:col-span-3"
              onClick={() => {
                setWho({ kind: "none" });
              }}
            >
              Tìm khách có sẵn
            </button>
          </fieldset>
        )}

        {packages.map((p, j) => {
          const pr = problems[j];
          const prefix = `packages[${String(j)}].`;
          const total = p.items.reduce(
            (n, i) => n + (Number(i.purchased) || 0) + (Number(i.bonus) || 0),
            0,
          );
          const price = parseAmount(p.price);
          return (
            <fieldset
              key={j}
              className="grid gap-3 rounded-[10px] border border-border p-3 sm:grid-cols-2"
            >
              <legend className="px-1 font-semibold">
                {String(j + 2)}. Liệu trình{" "}
                {packages.length > 1 ? String(j + 1) : ""}
              </legend>
              <label className="block sm:col-span-2">
                <span className="text-sm font-semibold">Gói</span>
                <select
                  className={input}
                  value={p.planId}
                  onChange={(e) => {
                    const plan = plans.data?.find(
                      (x) => x.id === e.target.value,
                    );
                    change(
                      j,
                      plan
                        ? fromPlan(p, plan, serviceName)
                        : {
                            ...emptyPackage(),
                            startsOn: p.startsOn,
                            expiresOn: p.expiresOn,
                            gift: p.gift,
                          },
                    );
                  }}
                >
                  <option value="">Gói khác (nhập tay)</option>
                  {plans.data?.map((plan) => (
                    <option key={plan.id} value={plan.id}>
                      {plan.name} — {formatVnd(plan.price)}
                      {plan.active ? "" : " (ngừng bán)"}
                    </option>
                  ))}
                </select>
              </label>
              {!p.planId && (
                <label className="block sm:col-span-2">
                  <span className="text-sm font-semibold">Tên gói</span>
                  <input
                    className={input}
                    value={p.name}
                    onChange={(e) => {
                      change(j, { ...p, name: e.target.value });
                    }}
                  />
                  <Err>{show(pr?.name) ?? server(`${prefix}name`)}</Err>
                </label>
              )}

              <div className="space-y-2 sm:col-span-2">
                {p.items.map((item, i) => (
                  <div key={i} className="flex flex-wrap items-end gap-2">
                    {p.planId ? (
                      <span className="min-w-40 flex-1 self-center font-medium">
                        {item.serviceName}
                      </span>
                    ) : (
                      <label className="min-w-40 flex-1">
                        <span className="text-sm font-semibold">Dịch vụ</span>
                        <select
                          className={input}
                          value={item.serviceId}
                          onChange={(e) => {
                            changeItem(j, i, { serviceId: e.target.value });
                          }}
                        >
                          <option value="">Chọn dịch vụ…</option>
                          {services.data?.map((s) => (
                            <option key={s.id} value={s.id}>
                              {s.name}
                            </option>
                          ))}
                        </select>
                      </label>
                    )}
                    <label>
                      <span className="block text-sm font-semibold">
                        Buổi mua
                      </span>
                      <input
                        className={small}
                        inputMode="numeric"
                        readOnly={Boolean(p.planId)}
                        value={item.purchased}
                        onChange={(e) => {
                          changeItem(j, i, { purchased: e.target.value });
                        }}
                      />
                    </label>
                    <label>
                      <span className="block text-sm font-semibold">Tặng</span>
                      <input
                        className={small}
                        inputMode="numeric"
                        readOnly={Boolean(p.planId)}
                        value={item.bonus}
                        onChange={(e) => {
                          changeItem(j, i, { bonus: e.target.value });
                        }}
                      />
                    </label>
                    <label>
                      <span className="block text-sm font-semibold">
                        Còn lại
                      </span>
                      <input
                        className={small}
                        inputMode="numeric"
                        value={item.remaining}
                        onChange={(e) => {
                          changeItem(j, i, { remaining: e.target.value });
                        }}
                      />
                    </label>
                    {!p.planId && p.items.length > 1 && (
                      <button
                        type="button"
                        className="min-h-11 rounded-[10px] border border-border px-3 text-sm"
                        onClick={() => {
                          change(j, {
                            ...p,
                            items: p.items.filter((_, k) => k !== i),
                          });
                        }}
                      >
                        Bỏ dịch vụ
                      </button>
                    )}
                    <span className="basis-full">
                      <Err>
                        {show(pr?.items[i]) ??
                          server(
                            `${prefix}items[${String(i)}].sessions_remaining`,
                          ) ??
                          server(`${prefix}items[${String(i)}].service_id`) ??
                          server(`${prefix}items[${String(i)}].sessions`)}
                      </Err>
                    </span>
                  </div>
                ))}
                {!p.planId && (
                  <button
                    type="button"
                    className="min-h-11 rounded-[10px] border border-border px-3 text-sm"
                    onClick={() => {
                      change(j, { ...p, items: [...p.items, emptyItem()] });
                    }}
                  >
                    + Thêm dịch vụ
                  </button>
                )}
                <Err>{server(`${prefix}items`)}</Err>
              </div>

              <label className="block">
                <span className="text-sm font-semibold">Giá đã bán (đ)</span>
                <input
                  className={input}
                  inputMode="numeric"
                  value={p.price}
                  onChange={(e) => {
                    change(j, { ...p, price: e.target.value });
                  }}
                />
                {!Number.isNaN(price) && total > 0 && (
                  <span className="block text-xs text-text-muted">
                    {formatVnd(price)} cho {String(total)} buổi
                  </span>
                )}
                <Err>{show(pr?.price) ?? server(`${prefix}sold_price`)}</Err>
              </label>
              <label className="block">
                <span className="text-sm font-semibold">Quà đã tặng</span>
                <input
                  className={input}
                  value={p.gift}
                  onChange={(e) => {
                    change(j, { ...p, gift: e.target.value });
                  }}
                />
              </label>
              <label className="block">
                <span className="text-sm font-semibold">Ngày đăng ký gốc</span>
                <input
                  className={input}
                  type="date"
                  max={today}
                  value={p.startsOn}
                  onChange={(e) => {
                    change(j, { ...p, startsOn: e.target.value });
                  }}
                />
                <Err>{show(pr?.startsOn) ?? server(`${prefix}starts_on`)}</Err>
              </label>
              <label className="block">
                <span className="text-sm font-semibold">Hạn dùng (nếu có)</span>
                <input
                  className={input}
                  type="date"
                  value={p.expiresOn}
                  onChange={(e) => {
                    change(j, { ...p, expiresOn: e.target.value });
                  }}
                />
                <Err>
                  {show(pr?.expiresOn) ?? server(`${prefix}expires_on`)}
                </Err>
              </label>
              {packages.length > 1 && (
                <button
                  type="button"
                  className="min-h-11 justify-self-start rounded-[10px] border border-border px-3 text-sm"
                  onClick={() => {
                    setPackages((all) => all.filter((_, k) => k !== j));
                  }}
                >
                  Xóa liệu trình này
                </button>
              )}
            </fieldset>
          );
        })}

        {packages.length < 10 && (
          <button
            type="button"
            className="min-h-11 rounded-[10px] border border-border px-3 text-sm font-medium"
            onClick={() => {
              setPackages((all) => [...all, emptyPackage()]);
            }}
          >
            + Thêm liệu trình khác cho khách này
          </button>
        )}

        {(plans.isError || services.isError) && (
          <p role="alert" className="text-danger">
            {messageFor(plans.error ?? services.error)}
          </p>
        )}
        {mutation.isError && (
          <p
            role="alert"
            className="rounded-[10px] bg-danger-soft p-3 text-sm text-danger"
          >
            {existingId ? (
              <>
                Số điện thoại này đã có hồ sơ.{" "}
                <button
                  type="button"
                  className="font-semibold underline"
                  onClick={() => {
                    const phone = who.kind === "new" ? who.phone : "";
                    setWho({
                      kind: "existing",
                      id: existingId,
                      label: `Khách đã có (SĐT ${phone})`,
                    });
                    mutation.reset();
                  }}
                >
                  Dùng hồ sơ đó
                </button>
              </>
            ) : (
              messageFor(mutation.error)
            )}
          </p>
        )}
        <button
          type="submit"
          disabled={mutation.isPending}
          className="min-h-11 rounded-[10px] bg-primary px-4 font-semibold text-on-primary disabled:opacity-60"
        >
          {mutation.isPending ? "Đang lưu…" : "Lưu và nhập tiếp"}
        </button>
      </form>
    </div>
  );
}
