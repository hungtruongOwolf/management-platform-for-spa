/** Thin fetch wrapper: same-origin cookies, JSON bodies, RFC 9457 problems → ApiError. */

export interface FieldProblem {
  field: string;
  code: string;
}

export class ApiError extends Error {
  constructor(
    readonly status: number,
    readonly code: string,
    readonly errors: FieldProblem[] = [],
  ) {
    super(`${String(status)} ${code}`);
    this.name = "ApiError";
  }

  /** Error code reported for one field (422 validation_failed), if any. */
  fieldCode(field: string): string | undefined {
    return this.errors.find((e) => e.field === field)?.code;
  }
}

interface Problem {
  code: string;
  errors?: FieldProblem[];
}

function isProblem(value: unknown): value is Problem {
  return typeof value === "object" && value !== null && typeof (value as Record<string, unknown>)["code"] === "string";
}

export interface RequestOptions {
  method?: string;
  body?: unknown;
  /** Extra headers, e.g. `{ "If-Match": "\"3\"" }` for optimistic updates. */
  headers?: Record<string, string>;
}

export async function api(path: string, init: RequestOptions = {}): Promise<unknown> {
  const headers: Record<string, string> = { ...(init.headers ?? {}) };
  if (init.body !== undefined) headers["Content-Type"] = "application/json";
  const response = await fetch(`/api/v1${path}`, {
    method: init.method ?? "GET",
    credentials: "same-origin",
    headers,
    ...(init.body === undefined ? {} : { body: JSON.stringify(init.body) }),
  });
  if (response.status === 204) return null;
  const text = await response.text();
  const data: unknown = text ? JSON.parse(text) : null;
  if (!response.ok) {
    if (isProblem(data)) throw new ApiError(response.status, data.code, data.errors ?? []);
    throw new ApiError(response.status, "error");
  }
  return data;
}

/** `If-Match` header for a record version read earlier. */
export function ifMatch(version: number): Record<string, string> {
  return { "If-Match": `"${String(version)}"` };
}

/** Only same-app paths may be used as post-login destinations (prevents open redirects). */
export function safeNextPath(next: unknown): string {
  if (typeof next !== "string" || !next.startsWith("/") || next.startsWith("//") || next.includes("\\")) {
    return "/";
  }
  return next;
}
