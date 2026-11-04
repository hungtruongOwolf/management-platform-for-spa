import type { QueryClient } from "@tanstack/react-query";
import { Outlet, createRootRouteWithContext, createRoute, createRouter, redirect, type RouterHistory } from "@tanstack/react-router";
import { LoginPage } from "../features/auth/LoginPage";
import { meQuery } from "../features/auth/session";
import { safeNextPath } from "../lib/api";
import { PlansPage } from "../features/catalog/PlansPage";
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
    typeof search["next"] === "string" ? { next: safeNextPath(search["next"]) } : {},
  beforeLoad: async ({ context, search }) => {
    const me = await context.queryClient.query({ ...meQuery, staleTime: "static" });
    // Already signed in: honour the (sanitised) destination instead of always going home.
    if (me) redirect({ href: safeNextPath(search.next), throw: true });
  },
  component: function Login() {
    const { next } = loginRoute.useSearch();
    return <LoginPage next={next} />;
  },
});

/** Everything below requires a session. This guard is UX only; the API enforces access. */
const authedRoute = createRoute({
  getParentRoute: () => rootRoute,
  id: "authed",
  beforeLoad: async ({ context, location }) => {
    const me = await context.queryClient.query({ ...meQuery, staleTime: "static" });
    if (!me) redirect({ to: "/dang-nhap", search: { next: location.href }, throw: true });
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

const routeTree = rootRoute.addChildren([loginRoute, authedRoute.addChildren([homeRoute, priceListRoute, plansRoute])]);

export function createAppRouter(queryClient: QueryClient, history?: RouterHistory) {
  return createRouter({ routeTree, context: { queryClient }, ...(history ? { history } : {}) });
}

declare module "@tanstack/react-router" {
  interface Register {
    router: ReturnType<typeof createAppRouter>;
  }
}
