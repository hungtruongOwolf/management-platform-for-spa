import { queryOptions } from "@tanstack/react-query";
import type {
  OpeningBalance,
  OpeningBalanceCreate,
  OpeningBalanceWindow,
  OpeningPackageCreate,
  PackagePlan,
} from "@vpyd/api-client";
import { api } from "../../lib/api";
import { parseAmount } from "../sales/api";

/** Whether the owner opened paper-book import for today's business day (EM decision M2). */
export const windowQuery = queryOptions({
  queryKey: ["opening-balance-window"],
  queryFn: async () =>
    (await api("/opening-balances/window")) as OpeningBalanceWindow,
});

/** Owner only: open (or close) paper-book import for today. */
export async function setWindow(open: boolean): Promise<OpeningBalanceWindow> {
  return (await api("/opening-balances/window", {
    method: "PUT",
    body: { open },
  })) as OpeningBalanceWindow;
}

/** One retry-safe import: the same key replays the first result instead of importing twice. */
export async function importOpeningBalance(
  body: OpeningBalanceCreate,
  idempotencyKey: string,
): Promise<OpeningBalance> {
  return (await api("/opening-balances", {
    method: "POST",
    body,
    headers: { "Idempotency-Key": idempotencyKey },
  })) as OpeningBalance;
}

/** A service line as typed: counts are text so a half-typed field is not coerced to 0. */
export interface ItemDraft {
  serviceId: string;
  /** Read-only label for plan items. */
  serviceName?: string;
  purchased: string;
  bonus: string;
  remaining: string;
}

export interface PackageDraft {
  /** "" = "Gói khác" (free-form). */
  planId: string;
  name: string;
  price: string;
  startsOn: string;
  expiresOn: string;
  gift: string;
  items: ItemDraft[];
}

export const emptyItem = (): ItemDraft => ({
  serviceId: "",
  purchased: "",
  bonus: "0",
  remaining: "",
});

export const emptyPackage = (): PackageDraft => ({
  planId: "",
  name: "",
  price: "",
  startsOn: "",
  expiresOn: "",
  gift: "",
  items: [emptyItem()],
});

/** Picking a plan copies its name, price and services; the manager then types what is left. */
export function fromPlan(
  draft: PackageDraft,
  plan: PackagePlan,
  serviceName: (id: string) => string | undefined,
): PackageDraft {
  return {
    ...draft,
    planId: plan.id,
    name: plan.name,
    price: String(plan.price),
    items: plan.items.map((i) => ({
      serviceId: i.service_id,
      serviceName: serviceName(i.service_id) ?? "Dịch vụ trong gói",
      purchased: String(i.sessions),
      bonus: String(i.bonus_sessions),
      remaining: "",
    })),
  };
}

const count = (text: string): number =>
  /^\d{1,3}$/.test(text.trim()) ? Number(text.trim()) : Number.NaN;

/** Mirrors the server rules (AC8.4) so mistakes show before saving; the server re-checks all of them. */
export function itemProblem(item: ItemDraft): string | undefined {
  const purchased = count(item.purchased);
  const bonus = item.bonus.trim() === "" ? 0 : count(item.bonus);
  const remaining = count(item.remaining);
  if (!item.serviceId) return "Chọn dịch vụ.";
  if (Number.isNaN(purchased) || Number.isNaN(bonus) || purchased + bonus === 0)
    return "Nhập tổng số buổi gốc.";
  if (Number.isNaN(remaining)) return "Nhập số buổi còn lại.";
  if (remaining > purchased + bonus)
    return `Còn lại không được lớn hơn tổng ${String(purchased + bonus)} buổi.`;
  return undefined;
}

export function packageProblems(p: PackageDraft, today: string) {
  const price = parseAmount(p.price);
  return {
    name: p.planId || p.name.trim() ? undefined : "Nhập tên gói.",
    price: Number.isNaN(price) ? "Nhập giá đã bán (chỉ chữ số)." : undefined,
    startsOn: !p.startsOn
      ? "Nhập ngày đăng ký gốc."
      : p.startsOn > today
        ? "Ngày đăng ký không được ở tương lai."
        : undefined,
    expiresOn:
      p.expiresOn && p.startsOn && p.expiresOn < p.startsOn
        ? "Hạn dùng trước ngày đăng ký."
        : undefined,
    items: p.items.map(itemProblem),
  };
}

export function isValid(problems: ReturnType<typeof packageProblems>): boolean {
  const { items, ...rest } = problems;
  return (
    Object.values(rest).every((v) => v === undefined) &&
    items.every((v) => v === undefined)
  );
}

export function toRequest(p: PackageDraft): OpeningPackageCreate {
  const gift = p.gift.trim();
  return {
    package_plan_id: p.planId || null,
    name: p.name.trim() || null,
    gift_note: gift === "" ? null : gift,
    sold_price: parseAmount(p.price),
    starts_on: p.startsOn,
    expires_on: p.expiresOn || null,
    items: p.items.map((i) => ({
      service_id: i.serviceId,
      sessions_purchased: count(i.purchased),
      sessions_bonus: i.bonus.trim() === "" ? 0 : count(i.bonus),
      sessions_remaining: count(i.remaining),
    })),
  };
}

/** Business "today" in the spa's zone (Asia/Ho_Chi_Minh), as YYYY-MM-DD. */
export function todayInVietnam(now: Date = new Date()): string {
  return new Intl.DateTimeFormat("en-CA", {
    timeZone: "Asia/Ho_Chi_Minh",
  }).format(now);
}
