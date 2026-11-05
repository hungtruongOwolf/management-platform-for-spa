import type { QueryClient } from "@tanstack/react-query";
import {
  Outlet,
  createRootRouteWithContext,
  createRoute,
  createRouter,
  lazyRouteComponent,
  redirect,
  type RouterHistory,
} from "@tanstack/react-router";
import { LoginPage } from "../features/auth/LoginPage";
import { authQuery } from "../features/auth/session";
import { customersSearch, historySearch } from "../features/customers/search";
import { FrontDeskPage } from "../features/front-desk/FrontDeskPage";
import { inactiveSearch, packagesSearch, reportsSearch } from "../features/reports/search";
import { staffSearch } from "../features/staff/search";
import { safeNextPath } from "../lib/api";
import { AppShell } from "./AppShell";
import { recordLocation } from "./trail";

// Other business screens are separate chunks so the first paint does not wait for their code
// (Lighthouse LCP budget, testing.md §4). The front desk stays in the main bundle: it is the screen
// kept open all day, and a lazy chunk would add a request after the shell to its LCP.
const customersModule = () => import("../features/customers/CustomersPage");
const reportsPage = () => import("../features/reports/ReportsPage");
const packagesPage = () => import("../features/reports/PackagesOverviewPage");
const inactivePage = () => import("../features/reports/InactiveCustomersPage");
const staffPage = () => import("../features/staff/StaffPage");

/** Shown while a lazy screen's chunk loads (after the router's pending delay): no spinner. */
function PageSkeleton() {
  return (
    <div role="status" aria-label="Đang tải màn hình" className="grid content-start gap-4">
      <div className="skeleton h-4 w-24" />
      <div className="skeleton h-8 w-64" />
      <div className="grid gap-4 sm:grid-cols-3">
        <div className="skeleton h-28" />
        <div className="skeleton h-28" />
        <div className="skeleton h-28" />
      </div>
      <div className="skeleton h-64" />
    </div>
  );
}

interface RouterContext {
  queryClient: QueryClient;
}

const rootRoute = createRootRouteWithContext<RouterContext>()({
  component: Outlet,
});

/** Stable Vietnamese URLs (deep-linkable, principle #10). */
const loginRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/dang-nhap",
  validateSearch: (search: Record<string, unknown>): { next?: string } =>
    typeof search["next"] === "string"
      ? { next: safeNextPath(search["next"]) }
      : {},
  beforeLoad: async ({ context, search }) => {
    const auth = await context.queryClient.query({
      ...authQuery,
      staleTime: "static",
    });
    // Already signed in: honour the (sanitised) destination instead of always going home.
    if (auth.state === "signed_in")
      redirect({ href: safeNextPath(search.next), throw: true });
    if (auth.state !== "anonymous")
      redirect({
        to: "/xac-thuc",
        search: search.next ? { next: search.next } : {},
        throw: true,
      });
  },
  component: function Login() {
    const { next } = loginRoute.useSearch();
    return <LoginPage next={next} />;
  },
});

/** Second login step (TOTP / recovery code, or forced enrollment for owners). UX only. */
const mfaRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/xac-thuc",
  validateSearch: (search: Record<string, unknown>): { next?: string } =>
    typeof search["next"] === "string"
      ? { next: safeNextPath(search["next"]) }
      : {},
  beforeLoad: async ({ context, search }) => {
    const auth = await context.queryClient.query({
      ...authQuery,
      staleTime: "static",
    });
    if (auth.state === "signed_in")
      redirect({ href: safeNextPath(search.next), throw: true });
    if (auth.state === "anonymous")
      redirect({
        to: "/dang-nhap",
        search: search.next ? { next: search.next } : {},
        throw: true,
      });
    return {
      step: auth.state === "mfa_enroll" ? "mfa_enroll" : "mfa_verify",
    } as const;
  },
  component: lazyRouteComponent(
    () => import("../features/auth/MfaPage"),
    "MfaRoutePage",
  ),
});

/** Everything below requires a session. This guard is UX only; the API enforces access. */
const authedRoute = createRoute({
  getParentRoute: () => rootRoute,
  id: "authed",
  beforeLoad: async ({ context, location }) => {
    const auth = await context.queryClient.query({
      ...authQuery,
      staleTime: "static",
    });
    if (auth.state === "anonymous")
      redirect({
        to: "/dang-nhap",
        search: { next: location.href },
        throw: true,
      });
    if (auth.state !== "signed_in")
      redirect({
        to: "/xac-thuc",
        search: { next: location.href },
        throw: true,
      });
  },
  component: AppShell,
});

const homeRoute = createRoute({
  getParentRoute: () => authedRoute,
  path: "/",
  component: lazyRouteComponent(() => import("./HomePage"), "HomePage"),
});

/*
 * List screens keep their state (filters, search, date) in the URL through a schema
 * (lib/searchState.ts): `validateSearch` drops bad and default values, and Back, reload or a shared
 * link reopen the list exactly as it was.
 */

/** Management views (report.read; the API enforces it). */
const reportsRoute = createRoute({
  getParentRoute: () => authedRoute,
  path: "/bao-cao",
  validateSearch: reportsSearch.validate,
  component: lazyRouteComponent(reportsPage, "ReportsPage"),
});

const packagesRoute = createRoute({
  getParentRoute: () => authedRoute,
  path: "/goi-lieu-trinh",
  validateSearch: packagesSearch.validate,
  component: lazyRouteComponent(packagesPage, "PackagesOverviewPage"),
});

const inactiveRoute = createRoute({
  getParentRoute: () => authedRoute,
  path: "/khach-lau-khong-quay-lai",
  validateSearch: inactiveSearch.validate,
  component: lazyRouteComponent(inactivePage, "InactiveCustomersPage"),
});

/** Staff management (spec staff-management); each tab's API enforces its own permission. */
const staffRoute = createRoute({
  getParentRoute: () => authedRoute,
  path: "/nhan-vien",
  validateSearch: staffSearch.validate,
  component: lazyRouteComponent(staffPage, "StaffPage"),
});

const settingsRoute = createRoute({
  getParentRoute: () => authedRoute,
  path: "/cai-dat",
  component: lazyRouteComponent(() => import("./SettingsPage"), "SettingsPage"),
});

const priceListRoute = createRoute({
  getParentRoute: () => authedRoute,
  path: "/bang-gia",
  component: lazyRouteComponent(
    () => import("../features/catalog/PriceListPage"),
    "PriceListPage",
  ),
});

const plansRoute = createRoute({
  getParentRoute: () => authedRoute,
  path: "/bang-gia/goi",
  component: lazyRouteComponent(
    () => import("../features/catalog/PlansPage"),
    "PlansPage",
  ),
});

/** The front desk (S7): the screen the manager-cashier keeps open all day. */
const frontDeskRoute = createRoute({
  getParentRoute: () => authedRoute,
  path: "/quay",
  component: FrontDeskPage,
});

const customersRoute = createRoute({
  getParentRoute: () => authedRoute,
  path: "/khach-hang",
  validateSearch: customersSearch.validate,
  component: lazyRouteComponent(customersModule, "CustomersPage"),
});

const customerRoute = createRoute({
  getParentRoute: () => authedRoute,
  path: "/khach-hang/$customerId",
  validateSearch: historySearch.validate,
  component: lazyRouteComponent(customersModule, "CustomerRoutePage"),
});

const routeTree = rootRoute.addChildren([
  loginRoute,
  mfaRoute,
  authedRoute.addChildren([
    homeRoute,
    priceListRoute,
    plansRoute,
    frontDeskRoute,
    customersRoute,
    customerRoute,
    reportsRoute,
    packagesRoute,
    inactiveRoute,
    staffRoute,
    settingsRoute,
  ]),
]);

export function createAppRouter(
  queryClient: QueryClient,
  history?: RouterHistory,
) {
  const router = createRouter({
    routeTree,
    context: { queryClient },
    defaultPendingComponent: PageSkeleton,
    // Back/Forward land where the user was on a list, not at its top.
    scrollRestoration: true,
    ...(history ? { history } : {}),
  });
  // The trail behind every back control (app/trail.ts).
  router.subscribe("onResolved", ({ toLocation }) => {
    recordLocation(toLocation.state.__TSR_index, toLocation.href, toLocation.pathname);
  });
  return router;
}

declare module "@tanstack/react-router" {
  interface Register {
    router: ReturnType<typeof createAppRouter>;
  }
}
