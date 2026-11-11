/**
 * URL state of /nhat-ky-thao-tac (lib/searchState.ts). The screen parses it itself, so this stays
 * in the lazy chunk. Filters carry ids and codes only, never a name or a phone (ADR-0021).
 */
import type { ActivityKind } from "@vpyd/api-client";
import { param, searchSchema, type Parser } from "../../lib/searchState";

export const ACTIVITY_KINDS = [
  "front_desk",
  "packages",
  "appointments",
  "customers",
  "staff",
  "payroll",
  "settings",
  "accounts",
  "other",
] as const satisfies readonly ActivityKind[];

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

/** A member's user id, `platform` or `system`; "" when absent or malformed (the URL is user input). */
const actor: Parser<string> = (raw) => {
  if (raw === "platform" || raw === "system") return raw;
  return typeof raw === "string" && UUID.test(raw) ? raw.toLowerCase() : "";
};

/** /nhat-ky-thao-tac?from=…&to=…&actor=…&kind=… — "" dates = the last 7 days (the API's default). */
export const activitySearch = searchSchema({
  from: param.day(),
  to: param.day(),
  actor,
  kind: param.oneOf<ActivityKind | "">(["", ...ACTIVITY_KINDS], ""),
});

export type ActivitySearch = ReturnType<typeof activitySearch.parse>;
