import { can, type Me } from "../features/auth/session";

/**
 * Where the management views (reports, package overview, absent customers, customer history) get
 * their data. TODO(management-views): the API ships on branch feat/management-views; once it is
 * merged and `pnpm api:gen` has the types, build with VITE_MANAGEMENT_DATA=api and delete the mock.
 *
 * - `api`  — real endpoints, gated by the server's `report.read` permission.
 * - `mock` — typed sample data with a visible "Dữ liệu mẫu" banner (dev, tests, and a staging
 *            preview built with VITE_MANAGEMENT_DATA=mock). Never invented numbers in production.
 * - `off`  — the default for production builds: the views and their nav items do not exist.
 */
export type ManagementData = "api" | "mock" | "off";

export function managementDataFrom(flag: string | undefined, dev: boolean): ManagementData {
  if (flag === "api" || flag === "mock" || flag === "off") return flag;
  return dev ? "mock" : "off";
}

export const MANAGEMENT_DATA = managementDataFrom(
  import.meta.env["VITE_MANAGEMENT_DATA"] as string | undefined,
  import.meta.env.DEV,
);

/**
 * UX gate of the management views (the server decides). With real data that is the API's
 * `report.read`; the mock stands in for it with `sale.read` (owner and manager), since today's
 * `/me` does not list `report.read` yet.
 */
export function canSeeManagement(me: Me | null, data: ManagementData = MANAGEMENT_DATA): boolean {
  if (data === "off") return false;
  return can(me, data === "api" ? "report.read" : "sale.read");
}
