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
import { FrontDeskPage } from "../features/front-desk/FrontDeskPage";
import { safeNextPath } from "../lib/api";
import {
  parseCatalogSearch,
  parseHistorySearch,
  parseInactiveSearch,
  parsePackagesSearch,
  parseReportsSearch,
} from "../features/reports/search";
import { AppShell } from "./AppShell";

// Other business screens are separate chunks so the first paint does not wait for their code
// (Lighthouse LCP budget, testing.md §4). The front desk stays in the main bundle: it is the screen
// kept open all day, and a lazy chunk would add a request after the shell to its LCP.
const customersModule = () => import("../features/customers/CustomersPage");
const reportsPage = () => import("../features/reports/ReportsPage");
const packagesPage = () => import("../features/reports/PackagesOverviewPage");
const inactivePage = () => import("../features/reports/InactiveCustomersPage");

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

/** Management views (app/features.ts): each page renders nothing useful unless the flag is on. */
const reportsRoute = createRoute({
  getParentRoute: () => authedRoute,
  path: "/bao-cao",
  validateSearch: parseReportsSearch,
  component: lazyRouteComponent(reportsPage, "ReportsPage"),
});

const packagesRoute = createRoute({
  getParentRoute: () => authedRoute,
  path: "/goi-lieu-trinh",
  validateSearch: parsePackagesSearch,
  component: lazyRouteComponent(packagesPage, "PackagesOverviewPage"),
});

const inactiveRoute = createRoute({
  getParentRoute: () => authedRoute,
  path: "/khach-lau-khong-quay-lai",
  validateSearch: parseInactiveSearch,
  component: lazyRouteComponent(inactivePage, "InactiveCustomersPage"),
});

const settingsRoute = createRoute({
  getParentRoute: () => authedRoute,
  path: "/cai-dat",
  component: lazyRouteComponent(() => import("./SettingsPage"), "SettingsPage"),
});

const priceListRoute = createRoute({
  getParentRoute: () => authedRoute,
  path: "/bang-gia",
  validateSearch: parseCatalogSearch,
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
  component: lazyRouteComponent(customersModule, "CustomersPage"),
});

const customerRoute = createRoute({
  getParentRoute: () => authedRoute,
  path: "/khach-hang/$customerId",
  validateSearch: parseHistorySearch,
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
    settingsRoute,
  ]),
]);

export function createAppRouter(
  queryClient: QueryClient,
  history?: RouterHistory,
) {
  return createRouter({
    routeTree,
    context: { queryClient },
    defaultPendingComponent: PageSkeleton,
    ...(history ? { history } : {}),
  });
}

declare module "@tanstack/react-router" {
  interface Register {
    router: ReturnType<typeof createAppRouter>;
  }
}
