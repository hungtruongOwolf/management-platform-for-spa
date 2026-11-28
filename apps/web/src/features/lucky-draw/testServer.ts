import { vi } from "vitest";
import type { DrawResult, Eligible, LuckyDrawDetail, LuckyDrawSummary, Participant, Prize } from "./types";

/**
 * An in-memory API that keeps the contract (architecture/lucky-draw.md §3) for the screen tests:
 * every write answers with the whole program, derived counts and status computed like the server's
 * domain. The spin picks the first eligible person (tests need to know who wins).
 */
export const OWNER = {
  user: { id: "u1", full_name: "Chủ Spa", phone: "0905123456" },
  active_tenant: { id: "t1", name: "Viên Phúc Y Đường" },
  tenants: [{ id: "t1", name: "Viên Phúc Y Đường" }],
  permissions: ["customer.read", "visit.read", "report.read", "lucky_draw.manage"],
};

export interface Call {
  method: string;
  path: string;
  headers: Record<string, string>;
  body: unknown;
}

interface Stored {
  id: string;
  name: string;
  created_at: string;
  version: number;
  participants: Participant[];
  prizes: Omit<Prize, "awarded" | "remaining" | "eligible_count">[];
  results: DrawResult[];
}

const fits = (eligible: Eligible, p: Participant) => eligible === "all" || eligible === p.gender;

export function view(d: Stored): LuckyDrawDetail {
  const out = new Set(d.results.map((r) => r.participant_id));
  const prizes: Prize[] = d.prizes.map((q) => {
    const awarded = d.results.filter((r) => r.prize_id === q.id && r.forfeited_at === null).length;
    return {
      ...q,
      awarded,
      remaining: q.quantity - awarded,
      eligible_count: d.participants.filter((p) => !out.has(p.id) && fits(q.eligible, p)).length,
    };
  });
  const open = prizes.some((q) => q.remaining > 0 && q.eligible_count > 0);
  const finished = d.results.length > 0 && !open;
  return {
    id: d.id,
    name: d.name,
    created_at: d.created_at,
    version: d.version,
    status: d.results.length === 0 ? "not_started" : finished ? "finished" : "in_progress",
    finished,
    participants: d.participants.map((p) => ({ ...p, result_id: d.results.find((r) => r.participant_id === p.id)?.id ?? null })),
    prizes,
    results: d.results,
  };
}

function summary(d: Stored): LuckyDrawSummary {
  const v = view(d);
  return {
    id: d.id,
    name: d.name,
    created_at: d.created_at,
    participant_count: d.participants.length,
    prize_slots: d.prizes.reduce((n, q) => n + q.quantity, 0),
    awarded: v.prizes.reduce((n, q) => n + q.awarded, 0),
    status: v.status,
  };
}

const problem = (status: number, code: string, errors: unknown[] = []) =>
  Response.json({ code, errors }, { status, headers: { "Content-Type": "application/problem+json" } });

export interface FakeApi {
  calls: Call[];
  draws: Map<string, Stored>;
  /** Answer the next matching request yourself (return undefined to let the fake handle it). */
  intercept: (fn: (call: Call) => Response | Promise<Response> | undefined) => void;
  /** Milliseconds the fake waits before answering a call (a slow server); 0 by default. */
  slow: (fn: (call: Call) => number) => void;
}

export function fakeApi(me: unknown = OWNER, initial: LuckyDrawDetail[] = []): FakeApi {
  const calls: Call[] = [];
  const draws = new Map<string, Stored>();
  // Copies: a test's writes never reach the fixtures another test starts from.
  for (const d of structuredClone(initial))
    draws.set(d.id, {
      ...d,
      prizes: d.prizes.map((q) => ({
        id: q.id,
        name: q.name,
        description: q.description,
        eligible: q.eligible,
        quantity: q.quantity,
        position: q.position,
        version: q.version,
      })),
    });
  let next = 100;
  // UUIDs like the API's; the prefix only tells the kinds apart when reading a failing test.
  const id = (p: string) => `${p === "d" ? "0193a5b0" : "00000000"}-0000-7000-8000-${String((next += 1)).padStart(12, "0")}`;
  const replays = new Map<string, Response>();
  let interceptor: ((call: Call) => Response | Promise<Response> | undefined) | undefined;
  let latency: (call: Call) => number = () => 0;

  const handle = (call: Call): Response => {
    const { method, path } = call;
    const body = (call.body ?? {}) as Record<string, unknown>;
    if (path === "/api/v1/me") return Response.json(me);
    if (path === "/api/v1/lucky-draws" && method === "GET")
      return Response.json({ items: [...draws.values()].reverse().map(summary) });
    if (path === "/api/v1/lucky-draws" && method === "POST") {
      const d: Stored = { id: id("d"), name: String(body["name"]), created_at: "2026-11-27T02:00:00Z", version: 1, participants: [], prizes: [], results: [] };
      draws.set(d.id, d);
      return Response.json(view(d), { status: 201 });
    }
    const m = /^\/api\/v1\/lucky-draws\/([^/]+)(?:\/(participants|prizes|spins|results)(?:\/([^/]+))?(?:\/(forfeit))?)?$/.exec(path);
    const d = m ? draws.get(decodeURIComponent(m[1] ?? "")) : undefined;
    if (!m || !d) return problem(404, "not_found");
    const [, , kind, sub] = m;
    const write = (status = 200) => {
      d.version += 1;
      return Response.json(view(d), { status });
    };
    if (!kind) {
      if (method === "GET") return Response.json(view(d), { headers: { ETag: `"${String(d.version)}"` } });
      if (method === "PUT") {
        if (call.headers["If-Match"] !== `"${String(d.version)}"`) return problem(412, "conflict.stale_version");
        d.name = String(body["name"]);
        return write();
      }
      if (method === "DELETE") {
        draws.delete(d.id);
        return new Response(null, { status: 204 });
      }
    }
    if (kind === "participants") {
      if (method === "POST") {
        for (const p of body["participants"] as { name: string; gender: Participant["gender"] }[])
          d.participants.push({ id: id("p"), name: p.name, gender: p.gender, position: d.participants.length, version: 1, result_id: null });
        return write(201);
      }
      const p = d.participants.find((x) => x.id === sub);
      if (!p) return problem(404, "not_found");
      if (d.results.some((r) => r.participant_id === p.id)) return problem(409, "lucky_draw.participant_drawn");
      if (method === "PUT") Object.assign(p, { name: body["name"], gender: body["gender"], version: p.version + 1 });
      else d.participants = d.participants.filter((x) => x !== p);
      return write();
    }
    if (kind === "prizes") {
      if (method === "POST") {
        d.prizes.push({
          id: id("q"),
          name: String(body["name"]),
          description: (body["description"] as string | null | undefined) ?? null,
          eligible: body["eligible"] as Eligible,
          quantity: Number(body["quantity"]),
          position: d.prizes.length,
          version: 1,
        });
        return write(201);
      }
      const q = d.prizes.find((x) => x.id === sub);
      if (!q) return problem(404, "not_found");
      if (method === "PUT") Object.assign(q, { ...body, version: q.version + 1 });
      else {
        if (d.results.some((r) => r.prize_id === q.id)) return problem(409, "lucky_draw.prize_drawn");
        d.prizes = d.prizes.filter((x) => x !== q);
      }
      return write();
    }
    if (kind === "spins" && method === "POST") {
      const key = call.headers["Idempotency-Key"] ?? "";
      const seen = replays.get(key);
      if (seen) return seen.clone();
      const q = view(d).prizes.find((x) => x.id === body["prize_id"]);
      if (!q) return problem(404, "not_found");
      if (q.remaining <= 0) return problem(409, "lucky_draw.prize_exhausted");
      const out = new Set(d.results.map((r) => r.participant_id));
      const winner = d.participants.find((p) => !out.has(p.id) && fits(q.eligible, p));
      if (!winner) return problem(409, "lucky_draw.no_eligible");
      const r: DrawResult = {
        id: id("r"),
        seq: d.results.length + 1,
        prize_id: q.id,
        participant_id: winner.id,
        drawn_at: "2026-11-27T03:15:00Z",
        forfeited_at: null,
      };
      d.results.push(r);
      d.version += 1;
      const plain = { id: r.id, seq: r.seq, prize_id: r.prize_id, participant_id: r.participant_id, drawn_at: r.drawn_at };
      const res = Response.json({ result: plain, draw: view(d) }, { status: 201 });
      replays.set(key, res.clone());
      return res;
    }
    if (kind === "results" && method === "POST") {
      const r = d.results.find((x) => x.id === sub);
      if (!r) return problem(404, "not_found");
      if (r.forfeited_at !== null) return problem(409, "lucky_draw.result_forfeited");
      r.forfeited_at = "2026-11-27T03:20:00Z";
      return write();
    }
    return problem(405, "error");
  };

  vi.spyOn(globalThis, "fetch").mockImplementation(async (input, init) => {
    const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    const call: Call = {
      method: init?.method ?? "GET",
      path: url.replace(/\?.*$/, ""),
      headers: (init?.headers ?? {}) as Record<string, string>,
      body: typeof init?.body === "string" ? JSON.parse(init.body) : undefined,
    };
    calls.push(call);
    const wait = latency(call);
    if (wait > 0)
      await new Promise((resolve) => {
        setTimeout(resolve, wait);
      });
    const custom = interceptor?.(call);
    if (custom) {
      interceptor = undefined;
      return custom;
    }
    return handle(call);
  });
  return {
    calls,
    draws,
    intercept: (fn) => {
      interceptor = fn;
    },
    slow: (fn) => {
      latency = fn;
    },
  };
}
