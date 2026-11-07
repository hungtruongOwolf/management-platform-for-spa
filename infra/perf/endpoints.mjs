// Signed-in latency per endpoint (perf page §2b): one kept-alive HTTP/2 connection, N sequential
// samples per endpoint, 230 ms apart (under the 5 rps limit). Prints p50/p95 and the API's own
// Server-Timing (db time, statement count, total server time) per endpoint.
//
//   node infra/perf/endpoints.mjs [samples=12]
//
// Env (file PERF_ENV_FILE, default .env.staging, or the environment):
//   PERF_PHONE (default the demo manager 0905000002), STAGING_MANAGER_LOGIN_PASSWORD, PERF_ORIGIN.
// A manager account is used because owners need TOTP.
import fs from "node:fs";
import { connect, json, loadEnv, origin, quantile, required, serverTiming, sleep, spaToday } from "./lib.mjs";

const env = loadEnv();
const base = origin(env);
const samples = Number(process.argv[2]) || 12;
const today = spaToday();
const month = today.slice(0, 7);
const day = new Date(`${today}T00:00:00Z`);
const monday = new Date(+day - ((day.getUTCDay() + 6) % 7) * 864e5).toISOString().slice(0, 10);
const sunday = new Date(+new Date(`${monday}T00:00:00Z`) + 6 * 864e5).toISOString().slice(0, 10);

const endpoints = [
  ["health", "/api/v1/health"],
  ["me", "/api/v1/me"],
  ["visits day (Quầy)", `/api/v1/visits?date=${today}`],
  ["customers q=nguyen", "/api/v1/customers?q=nguyen"],
  ["customers (recent)", "/api/v1/customers"],
  ["services", "/api/v1/services"],
  ["service-categories", "/api/v1/service-categories"],
  ["package-plans", "/api/v1/package-plans"],
  ["staff", "/api/v1/staff"],
  ["appointments day", `/api/v1/appointments?from=${today}&to=${today}`],
  ["appointments week", `/api/v1/appointments?from=${monday}&to=${sunday}`],
  ["reports daily", `/api/v1/reports/daily?date=${today}`],
  ["reports range 30d", "/api/v1/reports/range?preset=last_30_days"],
  ["attendance month", `/api/v1/attendance?month=${month}`],
  ["daily-reviews month", `/api/v1/daily-reviews?month=${month}`],
];

const api = await connect(base);
try {
  const login = await api.login(env.PERF_PHONE ?? "0905000002", required(env, "STAGING_MANAGER_LOGIN_PASSWORD"));
  if (login.status !== 200) throw new Error(`login answered ${login.status}`);
  const me = json(await api.request("GET", "/api/v1/me"));
  if (!me?.active_tenant) throw new Error("login did not open a spa session");

  const results = new Map();
  for (let i = 0; i < samples; i++) {
    for (const [name, p] of endpoints) {
      const r = await api.request("GET", p);
      const t = serverTiming(r.headers["server-timing"]);
      const row = results.get(name) ?? { status: r.status, ms: [], db: [], total: [], statements: t.statements, cache: r.headers["cache-control"] };
      row.ms.push(r.ms);
      if (t.db !== undefined) row.db.push(t.db);
      if (t.total !== undefined) row.total.push(t.total);
      row.status = r.status;
      results.set(name, row);
      await sleep(230);
    }
  }
  const rows = [...results].map(([endpoint, r]) => ({
    endpoint,
    status: r.status,
    p50: Math.round(quantile(r.ms, 0.5)),
    p95: Math.round(quantile(r.ms, 0.95)),
    "server p50": r.total.length ? +quantile(r.total, 0.5).toFixed(1) : "-",
    "db p50": r.db.length ? +quantile(r.db, 0.5).toFixed(1) : "-",
    "SQL/req": r.statements ?? "-",
    "cache-control": r.cache,
  }));
  console.table(rows);
  if (process.env.PERF_OUT) fs.writeFileSync(process.env.PERF_OUT, JSON.stringify({ base, today, samples, rows }, null, 1));
} finally {
  await api.close();
}
