// Shared helpers for the latency scripts. Credentials come from an env file read at run time
// (default: `.env.staging` at the repo root, override with PERF_ENV_FILE); nothing is printed,
// nothing goes through argv, and sessions are logged out at the end.
import fs from "node:fs";
import http2 from "node:http2";
import path from "node:path";
import { fileURLToPath } from "node:url";
import zlib from "node:zlib";

export const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

/** KEY=value lines of the env file (quotes stripped), merged under the process environment. */
export function loadEnv() {
  const file = process.env.PERF_ENV_FILE ?? path.join(ROOT, ".env.staging");
  const fromFile = fs.existsSync(file)
    ? Object.fromEntries(
        fs
          .readFileSync(file, "utf8")
          .split(/\r?\n/)
          .filter((l) => /^[A-Z_][A-Z0-9_]*=/.test(l))
          .map((l) => {
            const i = l.indexOf("=");
            return [l.slice(0, i), l.slice(i + 1).trim().replace(/^['"]|['"]$/g, "")];
          }),
      )
    : {};
  return { ...fromFile, ...process.env };
}

export function required(env, key) {
  const value = env[key];
  if (!value) {
    console.error(`missing ${key} (set it in the env file or the environment)`);
    process.exit(2);
  }
  return value;
}

/** Origin under test: PERF_ORIGIN (e.g. in .env.staging). No default: deployment URLs stay out of the public repo. */
export function origin(env) {
  return required(env, "PERF_ORIGIN");
}

/** Spa "today" (Asia/Ho_Chi_Minh, UTC+7) as YYYY-MM-DD. */
export function spaToday() {
  return new Date(Date.now() + 7 * 3600e3).toISOString().slice(0, 10);
}

export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

export function quantile(samples, p) {
  const s = [...samples].sort((a, b) => a - b);
  return s[Math.min(s.length - 1, Math.max(0, Math.ceil(p * s.length) - 1))];
}

/** `Server-Timing: db;dur=0.7;desc="14 statements", total;dur=3.1` → { db, statements, total }. */
export function serverTiming(header) {
  const out = {};
  for (const part of String(header ?? "").split(",")) {
    const [name, ...params] = part.trim().split(";");
    const dur = params.find((p) => p.trim().startsWith("dur="));
    if (dur) out[name] = Number(dur.split("=")[1]);
    const desc = params.find((p) => p.trim().startsWith("desc="));
    const n = desc?.match(/(\d+)/);
    if (name === "db" && n) out.statements = Number(n[1]);
  }
  return out;
}

/** One kept-alive HTTP/2 connection, like a browser tab, with the session cookie once logged in. */
export async function connect(base) {
  const session = http2.connect(base);
  await new Promise((resolve, reject) => {
    session.once("connect", resolve);
    session.once("error", reject);
  });
  let cookie = "";
  const request = (method, p, body) =>
    new Promise((resolve, reject) => {
      const started = performance.now();
      const headers = { ":method": method, ":path": p, "accept-encoding": "br, gzip", accept: "application/json", origin: base };
      if (cookie) headers.cookie = cookie;
      if (body) headers["content-type"] = "application/json";
      const r = session.request(headers);
      let head;
      const chunks = [];
      r.on("response", (h) => (head = h));
      r.on("data", (c) => chunks.push(c));
      r.on("end", () => resolve({ status: head[":status"], ms: performance.now() - started, headers: head, body: Buffer.concat(chunks) }));
      r.on("error", reject);
      if (body) r.write(JSON.stringify(body));
      r.end();
    });
  return {
    request,
    async login(phone, password) {
      const reply = await request("POST", "/api/v1/auth/login", { phone, password });
      cookie = (reply.headers["set-cookie"] ?? []).map((c) => c.split(";")[0]).join("; ");
      return reply;
    },
    async close() {
      if (cookie) await request("POST", "/api/v1/auth/logout").catch(() => undefined);
      session.close();
    },
  };
}

export function json(reply) {
  let b = reply.body;
  const enc = reply.headers["content-encoding"];
  if (enc === "br") b = zlib.brotliDecompressSync(b);
  else if (enc === "gzip") b = zlib.gunzipSync(b);
  try {
    return JSON.parse(b.toString());
  } catch {
    return null;
  }
}
