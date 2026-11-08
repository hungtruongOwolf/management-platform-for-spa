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
import { useSyncExternalStore } from "react";
import { LoginPage } from "../features/auth/LoginPage";
import { authQuery, authRetry, type Me } from "../features/auth/session";
import { customersSearch, historySearch } from "../features/customers/search";
import { frontDeskSearch } from "../features/front-desk/search";
import { FrontDeskPage } from "../features/front-desk/FrontDeskPage";
import { inactiveSearch, packagesSearch, reportsSearch } from "../features/reports/search";
import { myWorkSearch, staffSearch } from "../features/staff/search";
import { safeNextPath } from "../lib/api";
import { ForbiddenRoute, mayOpen } from "./access";
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
const myWorkPage = () => import("../features/staff/MyWorkPage");
const notesPage = () => import("../features/notes/NotesPage");

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

/** Before the session is known (`/me`, outside the shell). While a busy API (429) makes `/me`
 * wait, it says so calmly here instead of the error page (session.ts retries with backoff). */
function SessionPending() {
  const retrying = useSyncExternalStore(authRetry.subscribe, authRetry.get);
  return (
    <div className="mx-auto grid w-full max-w-6xl content-start gap-4 p-4 sm:p-8">
      {retrying && (
        <p role="status" className="note">
          Đang thử lại… Máy chủ đang bận, chờ vài giây.
        </p>
      )}
      <PageSkeleton />
    </div>
  );
}

interface RouterContext {
  queryClient: QueryClient;
}

/**
 * A screen's guard: a role that may not open it gets "Không có quyền" (StatusPage) instead of the
 * screen and a wall of 403s (EM live test: a technician on /quay). UX only; the API decides.
 */
function only(allowed: (me: Me) => boolean) {
  return async ({ context }: { context: RouterContext }) => {
    const auth = await context.queryClient.query({ ...authQuery, staleTime: "static" });
    if (auth.state === "signed_in" && !allowed(auth.me)) throw new ForbiddenRoute();
  };
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
    if (auth.state === "password_change") redirect({ to: "/doi-mat-khau", throw: true });
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
    if (auth.state === "password_change") redirect({ to: "/doi-mat-khau", throw: true });
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

/**
 * First sign-in with a temporary password (ADR-0016): the only screen until a new password is
 * set; the server refuses that session everywhere else (`auth.password_change_required`).
 */
const passwordChangeRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/doi-mat-khau",
  beforeLoad: async ({ context }) => {
    const auth = await context.queryClient.query({ ...authQuery, staleTime: "static" });
    if (auth.state === "signed_in") redirect({ to: "/", throw: true });
    if (auth.state !== "password_change") redirect({ to: "/dang-nhap", throw: true });
  },
  component: lazyRouteComponent(() => import("../features/auth/PasswordChange"), "PasswordChangePage"),
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
    if (auth.state === "password_change") redirect({ to: "/doi-mat-khau", throw: true });
    if (auth.state !== "signed_in")
      redirect({
        to: "/xac-thuc",
        search: { next: location.href },
        throw: true,
      });
  },
  pendingComponent: SessionPending,
  component: AppShell,
});

const homePage = () => import("./HomePage");

/*
 * Loaders start a screen's queries together with its chunk, from the URL alone (perf page §4d):
 * on a click, and earlier on hover or touch (`defaultPreload: "intent"`). They import the screen's
 * own module, so no feature code moves into the startup chunk, and they never wait for the data:
 * each panel keeps its own loading and error states.
 */
const homeRoute = createRoute({
  getParentRoute: () => authedRoute,
  path: "/",
  loader: ({ context }) => homePage().then((m) => {
      m.prefetchHome(context.queryClient);
    }),
  component: lazyRouteComponent(homePage, "HomePage"),
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
  beforeLoad: only(mayOpen.management),
  validateSearch: reportsSearch.validate,
  loaderDeps: ({ search }) => search,
  loader: ({ context, deps }) => reportsPage().then((m) => {
      m.prefetchReports(context.queryClient, deps);
    }),
  component: lazyRouteComponent(reportsPage, "ReportsPage"),
});

const packagesRoute = createRoute({
  getParentRoute: () => authedRoute,
  path: "/goi-lieu-trinh",
  beforeLoad: only(mayOpen.management),
  validateSearch: packagesSearch.validate,
  component: lazyRouteComponent(packagesPage, "PackagesOverviewPage"),
});

/**
 * "Nhật ký ghi chú" (spec management-views §10): report.read opens it; GET /notes decides per kind.
 * No `validateSearch`: the screen parses its URL state itself (features/notes/search.ts), so the
 * schema stays in its lazy chunk (startup JS budget ≤ 125 KB).
 */
const notesRoute = createRoute({
  getParentRoute: () => authedRoute,
  path: "/nhat-ky-ghi-chu",
  beforeLoad: only(mayOpen.management),
  component: lazyRouteComponent(notesPage, "NotesPage"),
});

const inactiveRoute = createRoute({
  getParentRoute: () => authedRoute,
  path: "/khach-lau-khong-quay-lai",
  beforeLoad: only(mayOpen.management),
  validateSearch: inactiveSearch.validate,
  component: lazyRouteComponent(inactivePage, "InactiveCustomersPage"),
});

/** Staff management (spec staff-management); each tab's API enforces its own permission. */
const staffRoute = createRoute({
  getParentRoute: () => authedRoute,
  path: "/nhan-vien",
  beforeLoad: only(mayOpen.staff),
  validateSearch: staffSearch.validate,
  component: lazyRouteComponent(staffPage, "StaffPage"),
});

/**
 * "Công và lương của tôi" for those whose /nhan-vien holds the staff books (a manager, EM
 * 2026-11-08); a technician's /nhan-vien already is this screen. `GET /me/*`, person from the session.
 */
const myPayRoute = createRoute({
  getParentRoute: () => authedRoute,
  path: "/nhan-vien/cua-toi",
  beforeLoad: only(mayOpen.myPay),
  validateSearch: staffSearch.validate,
  component: lazyRouteComponent(staffPage, "MyPayPage"),
});

/** A technician's own customers and bookings (spec staff-management §10 US9); `GET /me/work`. */
const myWorkRoute = createRoute({
  getParentRoute: () => authedRoute,
  path: "/khach-cua-toi",
  beforeLoad: only(mayOpen.myWork),
  validateSearch: myWorkSearch.validate,
  component: lazyRouteComponent(myWorkPage, "MyWorkPage"),
});

const settingsRoute = createRoute({
  getParentRoute: () => authedRoute,
  path: "/cai-dat",
  component: lazyRouteComponent(() => import("./SettingsPage"), "SettingsPage"),
});

const priceListRoute = createRoute({
  getParentRoute: () => authedRoute,
  path: "/bang-gia",
  beforeLoad: only(mayOpen.catalog),
  component: lazyRouteComponent(
    () => import("../features/catalog/PriceListPage"),
    "PriceListPage",
  ),
});

const plansRoute = createRoute({
  getParentRoute: () => authedRoute,
  path: "/bang-gia/goi",
  beforeLoad: only(mayOpen.catalog),
  component: lazyRouteComponent(
    () => import("../features/catalog/PlansPage"),
    "PlansPage",
  ),
});

/** The front desk (S7): the screen the manager-cashier keeps open all day. */
const frontDeskRoute = createRoute({
  getParentRoute: () => authedRoute,
  path: "/quay",
  beforeLoad: only(mayOpen.frontDesk),
  validateSearch: frontDeskSearch.validate,
  component: FrontDeskPage,
});

const customersRoute = createRoute({
  getParentRoute: () => authedRoute,
  path: "/khach-hang",
  beforeLoad: only(mayOpen.customers),
  validateSearch: customersSearch.validate,
  loader: ({ context }) => customersModule().then((m) => {
      m.prefetchCustomers(context.queryClient);
    }),
  component: lazyRouteComponent(customersModule, "CustomersPage"),
});

const customerRoute = createRoute({
  getParentRoute: () => authedRoute,
  path: "/khach-hang/$customerId",
  beforeLoad: only(mayOpen.customers),
  validateSearch: historySearch.validate,
  loaderDeps: ({ search }) => search,
  loader: ({ context, params, deps }) =>
    customersModule().then((m) => {
      m.prefetchCustomerPage(context.queryClient, params.customerId, deps);
    }),
  component: lazyRouteComponent(customersModule, "CustomerRoutePage"),
});

const routeTree = rootRoute.addChildren([
  loginRoute,
  mfaRoute,
  passwordChangeRoute,
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
    notesRoute,
    staffRoute,
    myPayRoute,
    myWorkRoute,
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
    // Hover or touch on a link starts its chunk and its queries (loaders above).
    defaultPreload: "intent",
    // TanStack Query owns freshness: the router always asks the loader, which reuses fresh cache.
    defaultPreloadStaleTime: 0,
    // Off the startup bundle: rare pages in one small chunk of their own.
    defaultNotFoundComponent: lazyRouteComponent(() => import("./StatusPage"), "NotFoundPage"),
    defaultErrorComponent: lazyRouteComponent(() => import("./StatusPage"), "ErrorPage"),
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
