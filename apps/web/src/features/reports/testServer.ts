import type {
  FollowUp,
  FollowUpCreate,
  InactiveCustomer,
} from "@vpyd/api-client";
import { vi } from "vitest";
import { spaToday } from "../../lib/format";
import { NEXT_TRY_DAYS } from "../customers/followUps";
import type { PackageFilter } from "./api";
import { shiftDays } from "./period";
import {
  mockCustomerStats,
  mockDaily,
  mockHistory,
  mockInactive,
  mockLedger,
  mockPackages,
  mockRange,
} from "./fixtures";

/** Test-only fake of the API for the management views, contract-shaped (fixtures.ts). */
export interface FakeOptions {
  permissions: string[];
  /** Drop every money field, as the server does without `report.revenue.read`. */
  withoutRevenue?: boolean;
  /** Reply of `GET /customers?q=` (customer search). */
  searchHits?: unknown[];
  /** Numbers `GET /customers/phone-check` reports as taken (digits only), with the owner's name. */
  takenPhones?: Record<string, { id: string; name: string }>;
  /** `/me.features`: the features the spa has on (absent: every feature on). */
  features?: string[];
}

export const ME = (permissions: string[]) => ({
  user: { id: "u1", full_name: "Quản lý Hà", phone: "0905 123 456" },
  active_tenant: { id: "t1", name: "Viên Phúc Y Đường" },
  tenants: [{ id: "t1", name: "Viên Phúc Y Đường" }],
  permissions,
});

/** Per-spy state: follow-ups recorded during the test. */
interface State {
  followUps: FollowUp[];
}

/** Same rule as the server (overview.rs): due = never contacted, or the planned day has come. */
function matchesFollowUp(
  c: InactiveCustomer,
  filter: string,
  today: string,
): boolean {
  if (filter === "not_contacted") return !c.last_follow_up;
  if (filter === "due")
    return (
      !c.last_follow_up ||
      (c.next_follow_up_on != null && c.next_follow_up_on <= today)
    );
  if (filter === "scheduled")
    return c.next_follow_up_on != null && c.next_follow_up_on > today;
  return true;
}

function route(
  method: string,
  url: URL,
  body: unknown,
  options: FakeOptions,
  state: State,
): { status: number; json: unknown } {
  const {
    permissions,
    withoutRevenue = false,
    searchHits = [],
    takenPhones = {},
    features,
  } = options;
  const ok = (json: unknown) => ({ status: 200, json });
  const q = url.searchParams;
  const p = url.pathname.replace(/^\/api\/v1/, "");
  const today = spaToday();
  if (p === "/me") return ok({ ...ME(permissions), ...(features ? { features } : {}) });
  if (p === "/customers" && q.has("q")) return ok(searchHits);
  if (p === "/customers/phone-check") {
    const digits = (q.get("phone") ?? "")
      .replace(/\D/g, "")
      .replace(/^84/, "0");
    const valid = /^0[35789]\d{8}$/.test(digits);
    const hit = valid ? takenPhones[digits] : undefined;
    return ok({
      valid,
      taken: hit !== undefined,
      existing_id: hit?.id ?? null,
      full_name: hit?.name ?? null,
    });
  }
  if (p === "/reports/daily") {
    const d = mockDaily(q.get("date") ?? today);
    if (withoutRevenue) {
      delete d.revenue;
      delete d.compare.yesterday.net_revenue;
      delete d.compare.same_weekday_last_week.net_revenue;
    }
    return ok(d);
  }
  if (p === "/reports/range") {
    const g = q.get("granularity");
    const r = mockRange(
      q.get("from") ?? today,
      q.get("to") ?? today,
      g === "week" || g === "month" || g === "year" ? g : "day",
    );
    if (withoutRevenue) {
      delete r.revenue;
      for (const b of r.series) delete b.revenue;
      for (const s of r.top_services) delete s.one_off_revenue;
      for (const s of r.top_packages) delete s.revenue;
    }
    return ok(r);
  }
  if (p === "/reports/ledger") {
    if (withoutRevenue)
      return {
        status: 403,
        json: {
          type: "about:blank",
          title: "Forbidden",
          status: 403,
          code: "auth.forbidden",
        },
      };
    return ok(mockLedger(q.get("from") ?? today, q.get("to") ?? today));
  }
  if (p === "/customers/inactive") {
    const page = mockInactive(
      today,
      Number(q.get("days") ?? "7"),
      state.followUps,
    );
    const kind = q.get("kind");
    const term = (q.get("q") ?? "").toLowerCase();
    const items = page.items
      .filter((c) => matchesFollowUp(c, q.get("follow_up") ?? "all", today))
      .filter((c) => !kind || c.kind === kind)
      .filter(
        (c) => q.get("scope") !== "with_sessions" || c.sessions_remaining > 0,
      )
      .filter(
        (c) =>
          !term ||
          c.full_name.toLowerCase().includes(term) ||
          (c.phone ?? "").includes(term),
      );
    // Server default (overview.rs): by due date (never contacted first) for due/scheduled, else by absence.
    const follow = q.get("follow_up") ?? "all";
    const sort =
      q.get("sort") ??
      (follow === "due" || follow === "scheduled" ? "due" : "absent");
    if (sort === "due")
      items.sort(
        (a, b) =>
          (a.next_follow_up_on ?? "").localeCompare(
            b.next_follow_up_on ?? "",
          ) || b.days_absent - a.days_absent,
      );
    // Two pages: the first half, then the rest behind a cursor.
    const half = Math.ceil(items.length / 2);
    return ok(
      q.get("cursor")
        ? { ...page, items: items.slice(half), next_cursor: null }
        : {
            ...page,
            items: items.slice(0, half),
            next_cursor: items.length > half ? "page-2" : null,
          },
    );
  }
  const followUps = /^\/customers\/([^/]+)\/follow-ups$/.exec(p);
  if (followUps) {
    const customerId = followUps[1] ?? "";
    if (method === "POST") {
      const b = body as FollowUpCreate;
      const contactedOn = b.contacted_on ?? today;
      // Like the server: `null` = "Không hẹn", only an omitted field takes the suggestion.
      const suggestedDays = NEXT_TRY_DAYS[b.outcome];
      const suggested = suggestedDays === null ? null : shiftDays(contactedOn, suggestedDays);
      const saved: FollowUp = {
        id: `fu-${String(state.followUps.length + 1)}`,
        customer_id: customerId,
        contacted_on: contactedOn,
        channel: b.channel,
        outcome: b.outcome,
        note: b.note ?? null,
        next_follow_up_on: b.next_follow_up_on === undefined ? suggested : b.next_follow_up_on,
        by_user_id: "u1",
        by_name: "Quản lý Hà",
        created_at: new Date().toISOString(),
      };
      state.followUps.unshift(saved);
      return { status: 201, json: saved };
    }
    return ok({
      items: state.followUps.filter((f) => f.customer_id === customerId),
      has_more: false,
    });
  }
  // Any customer id (lists link to mock-customer-N): one profile, so a test can click through.
  const profile =
    /^\/customers\/(?!inactive$)([^/]+)(\/history|\/summary)?$/.exec(p);
  if (profile) {
    const id = profile[1] ?? "c1";
    if (profile[2] === "/history") {
      const page = mockHistory(id, today);
      const term = (q.get("q") ?? "").toLowerCase();
      const kind = q.get("kind");
      const events = page.events
        .filter((e) => !kind || e.kind === kind)
        .filter(
          (e) => !q.get("from") || e.business_date >= (q.get("from") ?? ""),
        )
        .filter((e) => !q.get("to") || e.business_date <= (q.get("to") ?? ""))
        .filter(
          (e) =>
            !term ||
            (e.visit?.lines ?? []).some((l) =>
              `${l.service_name} ${l.category_name} ${l.staff_name} ${l.package_name ?? ""}`
                .toLowerCase()
                .includes(term),
            ) ||
            (e.purchase?.lines ?? []).some((l) =>
              l.description.toLowerCase().includes(term),
            ),
        );
      return ok({ ...page, events });
    }
    if (profile[2] === "/summary")
      return ok(mockCustomerStats(id, mockHistory(id, today)));
    return ok({
      id,
      full_name: "Nguyễn Thị Hà",
      phone: "0905 111 222",
      gender: "female",
      version: 1,
    });
  }
  if (p === "/packages") {
    const filter: PackageFilter = q.get("expiring_within_days")
      ? "expiring"
      : ((q.get("status") as PackageFilter | null) ?? "all");
    return ok(mockPackages(today, filter, q.get("q") ?? ""));
  }
  return ok([]);
}

/** Replaces `fetch`; returns the spy so a test can inspect the requests (URL and init). */
export function serveManagement(options: FakeOptions) {
  const state: State = { followUps: [] };
  return vi.spyOn(globalThis, "fetch").mockImplementation((input, init) => {
    const raw =
      typeof input === "string"
        ? input
        : input instanceof URL
          ? input.href
          : input.url;
    const method =
      init?.method ?? (input instanceof Request ? input.method : "GET");
    const body: unknown =
      typeof init?.body === "string" ? JSON.parse(init.body) : undefined;
    const { status, json } = route(
      method.toUpperCase(),
      new URL(raw, "http://app.test"),
      body,
      options,
      state,
    );
    return Promise.resolve(
      Response.json(json, {
        status,
        headers:
          status >= 400 ? { "content-type": "application/problem+json" } : {},
      }),
    );
  });
}
