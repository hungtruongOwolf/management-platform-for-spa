/** Thin fetch wrapper: same-origin cookies, JSON bodies, RFC 9457 problems → ApiError. */

/** What any failed request that never reached the server says (lib/errors.ts `messageFor`). */
export const NETWORK_MESSAGE = "Không kết nối được máy chủ. Vui lòng kiểm tra mạng.";

export interface FieldProblem {
  field: string;
  code: string;
}

export class ApiError extends Error {
  constructor(
    readonly status: number,
    readonly code: string,
    readonly errors: FieldProblem[] = [],
    /** Id of the record a 409 conflicts with (e.g. the customer who already has this phone). */
    readonly existingId?: string,
    /** Seconds the server asks to wait before trying again (`Retry-After` of a 429/503). */
    readonly retryAfter?: number,
    /** The records behind a refusal, when the code documents them (e.g. the bookings of `staff.has_upcoming_bookings`). */
    readonly details?: unknown,
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
  existing_id?: string;
  details?: unknown;
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
    const wait = Number(response.headers.get("retry-after"));
    const retryAfter = Number.isFinite(wait) && wait > 0 ? wait : undefined;
    if (isProblem(data)) throw new ApiError(response.status, data.code, data.errors ?? [], data.existing_id, retryAfter, data.details);
    throw new ApiError(response.status, "error", [], undefined, retryAfter);
  }
  return data;
}

/** `If-Match` header for a record version read earlier. */
export function ifMatch(version: number): Record<string, string> {
  return { "If-Match": `"${String(version)}"` };
}

/**
 * Control characters, space and backslash. URL parsers strip tab/CR/LF and treat "\" as "/",
 * so "/\t/evil.example" would turn into the protocol-relative "//evil.example".
 */
// eslint-disable-next-line no-control-regex
const UNSAFE_PATH_CHARS = /[\x00-\x20\\]/;

/** Only same-app paths may be used as post-login destinations (prevents open redirects). */
export function safeNextPath(next: unknown): string {
  if (typeof next !== "string" || !next.startsWith("/") || next.startsWith("//") || UNSAFE_PATH_CHARS.test(next)) {
    return "/";
  }
  return next;
}
