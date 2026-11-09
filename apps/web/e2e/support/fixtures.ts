import { expect, type APIRequestContext } from "@playwright/test";
import { stack } from "../stack";

/**
 * Test data through the owner's API, shared by the specs (one copy, not one per spec).
 * Every write carries `WRITE`: the API refuses a cookie-authenticated write without the app's
 * `Origin` (CSRF guard, ADR-0004).
 */
export const WRITE = { Origin: stack.webOrigin } as const;

/** Today (plus `offsetDays`) in the spa's time zone (UTC+7, no daylight saving), "YYYY-MM-DD". */
export function spaDay(offsetDays = 0): string {
  return new Date(Date.now() + 7 * 3_600_000 + offsetDays * 86_400_000).toISOString().slice(0, 10);
}

export interface Service {
  id: string;
  name: string;
  active: boolean;
  duration_min: number | null;
  prices: { all?: number | null };
}

/** A seeded service on sale at one price for everyone (no Nữ / Nam split). */
export async function flatService(request: APIRequestContext): Promise<Service> {
  const services = (await (await request.get("/api/v1/services")).json()) as Service[];
  const service = services.find((s) => s.active && s.prices.all != null);
  expect(service, "seeded menu has a flat-priced service").toBeDefined();
  return service as Service;
}

export interface PlanOptions {
  name: string;
  price: number;
  /** Default 6. */
  validityMonths?: number;
  giftNote?: string | null;
  /** Default: the first flat-priced service. */
  serviceId?: string;
}

/** A "10 + 2 bonus" plan of one service; returns its id and name. */
export async function createPlan(request: APIRequestContext, plan: PlanOptions): Promise<{ id: string; name: string }> {
  const serviceId = plan.serviceId ?? (await flatService(request)).id;
  const reply = await request.post("/api/v1/package-plans", {
    headers: WRITE,
    data: {
      name: plan.name,
      price: plan.price,
      validity_months: plan.validityMonths ?? 6,
      gift_note: plan.giftNote ?? null,
      active: true,
      items: [{ service_id: serviceId, sessions: 10, bonus_sessions: 2 }],
    },
  });
  expect(reply.status()).toBe(201);
  return { id: ((await reply.json()) as { id: string }).id, name: plan.name };
}
