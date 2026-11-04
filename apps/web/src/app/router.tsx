import type { QueryClient } from "@tanstack/react-query";
import {
  Outlet,
  createRootRouteWithContext,
  createRoute,
  createRouter,
  redirect,
  type RouterHistory,
} from "@tanstack/react-router";
import { LoginPage } from "../features/auth/LoginPage";
import { MfaPage } from "../features/auth/MfaPage";
import { authQuery } from "../features/auth/session";
import { safeNextPath } from "../lib/api";
import { PlansPage } from "../features/catalog/PlansPage";
import {
  CustomerPage,
  CustomersPage,
} from "../features/customers/CustomersPage";
import { PriceListPage } from "../features/catalog/PriceListPage";
import { AppShell, HomePage } from "./AppShell";

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
      redirect({ to: "/xac-thuc", search: search.next ? { next: search.next } : {}, throw: true });
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
      redirect({ to: "/dang-nhap", search: search.next ? { next: search.next } : {}, throw: true });
    return { step: auth.state === "mfa_enroll" ? "mfa_enroll" : "mfa_verify" } as const;
  },
  component: function Mfa() {
    const { next } = mfaRoute.useSearch();
    const { step } = mfaRoute.useRouteContext();
    return <MfaPage step={step} next={next} />;
  },
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
  component: HomePage,
});

const priceListRoute = createRoute({
  getParentRoute: () => authedRoute,
  path: "/bang-gia",
  component: PriceListPage,
});

const plansRoute = createRoute({
  getParentRoute: () => authedRoute,
  path: "/bang-gia/goi",
  component: PlansPage,
});

const customersRoute = createRoute({
  getParentRoute: () => authedRoute,
  path: "/khach-hang",
  component: CustomersPage,
});

const customerRoute = createRoute({
  getParentRoute: () => authedRoute,
  path: "/khach-hang/$customerId",
  component: function Customer() {
    const { customerId } = customerRoute.useParams();
    return <CustomerPage customerId={customerId} />;
  },
});

const routeTree = rootRoute.addChildren([
  loginRoute,
  mfaRoute,
  authedRoute.addChildren([
    homeRoute,
    priceListRoute,
    plansRoute,
    customersRoute,
    customerRoute,
  ]),
]);

export function createAppRouter(
  queryClient: QueryClient,
  history?: RouterHistory,
) {
  return createRouter({
    routeTree,
    context: { queryClient },
    ...(history ? { history } : {}),
  });
}

declare module "@tanstack/react-router" {
  interface Register {
    router: ReturnType<typeof createAppRouter>;
  }
}
