/**
 * First screens that live in their own chunk, not in the startup chunk (frontend.md "Perf
 * budget"). One loader per screen, shared by the router (`lazyRouteComponent`) and every place
 * that starts the chunk early, so they all get the same module.
 *
 * Opening the app on one of these paths does not wait for the startup chunk before fetching the
 * screen: the build adds a small script that starts the landing screen's chunk next to it
 * (`ROUTE_PRELOAD` in vite.config.ts lists the same paths; `startup-budget.mjs` gates each one).
 */
export const loadFrontDesk = () => import("../features/front-desk/FrontDeskPage");
export const loadHome = () => import("./HomePage");

/** Starts the chunk of the screen `href` opens (e.g. where sign-in goes next), without waiting. */
export function preloadScreen(href: string): void {
  const path = href.split(/[?#]/, 1)[0];
  if (path === "/quay") void loadFrontDesk();
  else if (path === "/") void loadHome();
}
