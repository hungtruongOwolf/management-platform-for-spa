/**
 * p95 latency of the hot read endpoints against seeded data (spec §6: customer search < 300 ms p95).
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
import { credentials } from "../support/run";

const SEEDED_CUSTOMERS = Number(process.env["PERF_CUSTOMERS"] ?? 300);
const SAMPLES = Number(process.env["PERF_SAMPLES"] ?? 100);
/** Spec §6 budget for search; catalog reads use the same until a tighter one is set. */
const BUDGET_MS = 300;

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
  const { phone, password } = credentials();
  const login = await api.post("/api/v1/auth/login", {
    data: { phone, password },
    headers: { "X-Forwarded-For": nextIp() },
  });
  expect(login.status()).toBe(200);
  // The cookie is `Secure`; pass it explicitly instead of relying on the jar over plain http.
  const cookie = (login.headers()["set-cookie"] ?? "").split(";")[0] ?? "";
  return { api, cookie };
}

test("hot read endpoints stay under the p95 budget", async () => {
  test.setTimeout(5 * 60_000);
  const { api, cookie } = await signedIn();
  const headers = () => ({ Cookie: cookie, "X-Forwarded-For": nextIp() });

  for (let i = 0; i < SEEDED_CUSTOMERS; i += 1) {
    const name = `${FAMILY[i % FAMILY.length] ?? ""} ${MIDDLE[i % MIDDLE.length] ?? ""} ${GIVEN[i % GIVEN.length] ?? ""}`;
    const response = await api.post("/api/v1/customers", {
      headers: headers(),
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
