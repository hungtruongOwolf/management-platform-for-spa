/** Thin fetch wrapper: same-origin cookies, JSON bodies, RFC 9457 problems → ApiError. */

export class ApiError extends Error {
  constructor(
    readonly status: number,
    readonly code: string,
  ) {
    super(`${String(status)} ${code}`);
    this.name = "ApiError";
  }
}

interface Problem {
  code: string;
}

function isProblem(value: unknown): value is Problem {
  return typeof value === "object" && value !== null && typeof (value as Record<string, unknown>)["code"] === "string";
}

export async function api(path: string, init: { method?: string; body?: unknown } = {}): Promise<unknown> {
  const response = await fetch(`/api/v1${path}`, {
    method: init.method ?? "GET",
    credentials: "same-origin",
    headers: init.body === undefined ? {} : { "Content-Type": "application/json" },
    ...(init.body === undefined ? {} : { body: JSON.stringify(init.body) }),
  });
  if (response.status === 204) return null;
  const text = await response.text();
  const data: unknown = text ? JSON.parse(text) : null;
  if (!response.ok) {
    throw new ApiError(response.status, isProblem(data) ? data.code : "error");
  }
  return data;
}

/** Only same-app paths may be used as post-login destinations (prevents open redirects). */
export function safeNextPath(next: unknown): string {
  if (typeof next !== "string" || !next.startsWith("/") || next.startsWith("//") || next.includes("\\")) {
    return "/";
  }
  return next;
}
