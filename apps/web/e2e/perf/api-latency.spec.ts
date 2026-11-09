/**
 * p95 latency of the hot endpoints against seeded data (spec §6: customer search < 300 ms p95,
 * saving a visit < 500 ms p95; other reads use the search budget, other money writes the save one).
 * Talks to the API directly (no Vite proxy) so the number is the server's. Sequential single
 * client: a regression detector for slow queries, not a load test (k6 belongs to S11 staging).
 *
 * Each request carries its own X-Forwarded-For so the per-IP rate limiter does not turn the
 * measurement into a measurement of 429s; the server trusts that header only behind Caddy.
 */
import {
  expect,
  request,
  test,
  type APIRequestContext,
} from "@playwright/test";
import { writeFileSync } from "node:fs";
import { stack } from "../stack";
import { apiSignIn } from "../support/run";

const SEEDED_CUSTOMERS = Number(process.env["PERF_CUSTOMERS"] ?? 300);
const SAMPLES = Number(process.env["PERF_SAMPLES"] ?? 100);
/** Spec §6 budget for search; catalog reads use the same until a tighter one is set. */
const BUDGET_MS = 300;
/** Spec §6 budget for saving a visit; selling a package and importing a paper-book one use it too. */
const WRITE_BUDGET_MS = 500;

const FAMILY = [
  "Nguyễn",
  "Trần",
  "Lê",
  "Phạm",
  "Hoàng",
  "Huỳnh",
  "Phan",
  "Võ",
  "Đặng",
  "Bùi",
];
const MIDDLE = ["Thị", "Văn", "Ngọc", "Minh", "Thu", "Hồng"];
const GIVEN = [
  "Hà",
  "Lan",
  "Hoà",
  "Đức",
  "Phúc",
  "Yến",
  "Tâm",
  "Thảo",
  "Hùng",
  "Nga",
  "Trang",
];

let ip = 0;
function nextIp(): string {
  ip += 1;
  return `10.77.${String((ip >> 8) & 0xff)}.${String(ip & 0xff)}`;
}

function p95(samples: number[]): number {
  const sorted = [...samples].sort((a, b) => a - b);
  return (
    sorted[Math.min(sorted.length - 1, Math.ceil(sorted.length * 0.95) - 1)] ??
    Number.NaN
  );
}

async function signedIn(): Promise<{ api: APIRequestContext; cookie: string }> {
  const api = await request.newContext({
    baseURL: stack.apiOrigin,
    extraHTTPHeaders: { Origin: stack.webOrigin },
  });
  // The cookie is `Secure`; pass it explicitly instead of relying on the jar over plain http.
  const cookie = await apiSignIn(async (path, data, previous) => {
    const reply = await api.post(path, {
      data,
      headers: {
        "X-Forwarded-For": nextIp(),
        ...(previous ? { Cookie: previous } : {}),
      },
    });
    return {
      status: reply.status(),
      cookie: (reply.headers()["set-cookie"] ?? "").split(";")[0] ?? "",
      body: (await reply.json()) as unknown,
    };
  });
  return { api, cookie };
}

test("hot read endpoints stay under the p95 budget", async () => {
  test.setTimeout(5 * 60_000);
  const { api, cookie } = await signedIn();
  const headers = (extra: Record<string, string> = {}) => ({ Cookie: cookie, "X-Forwarded-For": nextIp(), ...extra });

  for (let i = 0; i < SEEDED_CUSTOMERS; i += 1) {
    const name = `${FAMILY[i % FAMILY.length] ?? ""} ${MIDDLE[i % MIDDLE.length] ?? ""} ${GIVEN[i % GIVEN.length] ?? ""}`;
    const response = await api.post("/api/v1/customers", {
      headers: headers({ "Idempotency-Key": crypto.randomUUID() }),
      data: {
        full_name: name,
        phone: `0935${String(100_000 + i)}`,
        gender: i % 3 === 0 ? "male" : "female",
      },
    });
    // 409 = already seeded by an earlier attempt of this run.
    expect([201, 409]).toContain(response.status());
  }

  const endpoints = [
    "/api/v1/customers?q=nguyen%20thi",
    "/api/v1/customers?q=hoa",
    "/api/v1/customers?q=0935100",
    "/api/v1/customers",
    "/api/v1/service-categories",
    "/api/v1/services",
    "/api/v1/package-plans",
  ];
  const report: Record<
    string,
    { p95: number; median: number; samples: number }
  > = {};
  for (const path of endpoints) {
    await api.get(path, { headers: headers() }); // warm-up (connection, plan cache)
    const timings: number[] = [];
    for (let i = 0; i < SAMPLES; i += 1) {
      const started = performance.now();
      const response = await api.get(path, { headers: headers() });
      timings.push(performance.now() - started);
      expect(response.status(), path).toBe(200);
    }
    const sorted = [...timings].sort((a, b) => a - b);
    report[path] = {
      p95: Math.round(p95(timings) * 10) / 10,
      median:
        Math.round((sorted[Math.floor(sorted.length / 2)] ?? 0) * 10) / 10,
      samples: SAMPLES,
    };
  }

  const body = JSON.stringify(
    { budgetMs: BUDGET_MS, seededCustomers: SEEDED_CUSTOMERS, report },
    null,
    2,
  );
  writeFileSync(test.info().outputPath("api-p95.json"), body);
  await test
    .info()
    .attach("api-p95.json", { body, contentType: "application/json" });
  console.log(`API latency (ms):\n${body}`);

  const overBudget = Object.entries(report)
    .filter(([, r]) => r.p95 > BUDGET_MS)
    .map(([path, r]) => `${path}: p95 ${String(r.p95)} ms`);
  expect(overBudget, `p95 over ${String(BUDGET_MS)} ms`).toEqual([]);
  await api.dispose();
});

interface Timing {
  p95: number;
  median: number;
  samples: number;
}

function summarize(timings: number[]): Timing {
  const sorted = [...timings].sort((a, b) => a - b);
  return {
    p95: Math.round(p95(timings) * 10) / 10,
    median: Math.round((sorted[Math.floor(sorted.length / 2)] ?? 0) * 10) / 10,
    samples: timings.length,
  };
}

async function report(
  name: string,
  budgets: Record<string, number>,
  results: Record<string, Timing>,
): Promise<void> {
  const body = JSON.stringify({ budgets, results }, null, 2);
  writeFileSync(test.info().outputPath(name), body);
  await test.info().attach(name, { body, contentType: "application/json" });
  console.log(`API latency (ms):
${body}`);
  const overBudget = Object.entries(results)
    .filter(([path, r]) => r.p95 > (budgets[path] ?? BUDGET_MS))
    .map(([path, r]) => `${path}: p95 ${String(r.p95)} ms`);
  expect(overBudget, "p95 over budget").toEqual([]);
}

test("front-desk writes and the reads around them stay under budget", async () => {
  test.setTimeout(5 * 60_000);
  const { api, cookie } = await signedIn();
  const headers = (extra: Record<string, string> = {}) => ({
    Cookie: cookie,
    "X-Forwarded-For": nextIp(),
    ...extra,
  });
  const json = async <T>(path: string): Promise<T> =>
    (await (await api.get(path, { headers: headers() })).json()) as T;

  const services =
    await json<{ id: string; active: boolean; prices: { all?: number } }[]>(
      "/api/v1/services",
    );
  const service = services.find((s) => s.active && s.prices.all != null);
  const staff = (await json<{ id: string }[]>("/api/v1/staff"))[0];
  expect(service, "seeded menu has a flat-priced service").toBeDefined();
  expect(staff, "setup added a technician").toBeDefined();
  const price = service?.prices.all ?? 0;

  const plan = await api.post("/api/v1/package-plans", {
    headers: headers(),
    data: {
      name: `Gói đo hiệu năng ${String(Date.now())}`,
      price: 5_000_000,
      validity_months: null,
      gift_note: null,
      active: true,
      items: [{ service_id: service?.id, sessions: 500, bonus_sessions: 0 }],
    },
  });
  expect(plan.status()).toBe(201);
  const planId = ((await plan.json()) as { id: string }).id;
  const customer = await api.post("/api/v1/customers", {
    headers: headers({ "Idempotency-Key": crypto.randomUUID() }),
    data: {
      full_name: "Khách Đo Hiệu Năng",
      phone: `0936${String(Date.now()).slice(-6)}`,
      gender: "female",
    },
  });
  expect(customer.status()).toBe(201);
  const customerId = ((await customer.json()) as { id: string }).id;

  const sell = () => ({
    customer_id: customerId,
    closer_staff_id: staff?.id,
    lines: [{ package_plan_id: planId }],
    payments: [{ method: "cash", amount: 5_000_000 }],
  });
  const visit = () => ({
    customer_id: customerId,
    lines: [
      { service_id: service?.id, source: "package", staff_id: staff?.id },
      { service_id: service?.id, source: "one_off", staff_id: staff?.id },
    ],
    payments: [{ method: "bank_transfer", amount: price }],
  });
  const openingBalance = () => ({
    customer_id: customerId,
    packages: [
      {
        name: "Gói sổ cũ (đo)",
        sold_price: 1_500_000,
        starts_on: "2026-06-01",
        items: [
          {
            service_id: service?.id,
            sessions_purchased: 10,
            sessions_bonus: 0,
            sessions_remaining: 4,
          },
        ],
      },
    ],
  });
  const writes: [string, () => unknown][] = [
    ["POST /api/v1/sales", sell],
    ["POST /api/v1/visits", visit],
    ["POST /api/v1/opening-balances", openingBalance],
  ];
  const results: Record<string, Timing> = {};
  const budgets: Record<string, number> = {};
  let saleId = "";
  for (const [name, body] of writes) {
    const path = name.replace("POST ", "");
    const timings: number[] = [];
    for (let i = 0; i <= SAMPLES; i += 1) {
      const started = performance.now();
      const response = await api.post(path, {
        headers: headers({ "Idempotency-Key": crypto.randomUUID() }),
        data: body(),
      });
      const elapsed = performance.now() - started;
      expect(response.status(), `${name}: ${await response.text()}`).toBe(201);
      if (i > 0) timings.push(elapsed); // the first one warms up
      if (path.endsWith("/sales") && !saleId) {
        saleId = ((await response.json()) as { id: string }).id;
      }
    }
    results[name] = summarize(timings);
    budgets[name] = WRITE_BUDGET_MS;
  }

  // Reads of a busy day: >100 visits today, a customer with >200 packages.
  const reads = [
    "/api/v1/visits",
    `/api/v1/customers/${customerId}`,
    `/api/v1/customers/${customerId}/packages`,
    `/api/v1/sales/${saleId}`,
    "/api/v1/staff",
  ];
  for (const path of reads) {
    await api.get(path, { headers: headers() });
    const timings: number[] = [];
    for (let i = 0; i < SAMPLES; i += 1) {
      const started = performance.now();
      const response = await api.get(path, { headers: headers() });
      timings.push(performance.now() - started);
      expect(response.status(), path).toBe(200);
    }
    const key = `GET ${path.replace(customerId, "{id}").replace(saleId, "{id}")}`;
    results[key] = summarize(timings);
    budgets[key] = BUDGET_MS;
  }
  await report("api-p95-front-desk.json", budgets, results);
  await api.dispose();
});
