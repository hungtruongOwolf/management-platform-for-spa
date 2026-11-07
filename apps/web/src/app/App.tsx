import { QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider, type RouterHistory } from "@tanstack/react-router";
import { useEffect, useState } from "react";
import { afterFirstPaint } from "../lib/afterFirstPaint";
import { createQueryClient } from "../lib/queryClient";
import { declareDeferredHead } from "./deferredHead";
import { loadFontsAfterFirstScreen, loadFontsIfCached } from "./fonts";
import { createAppRouter } from "./router";

/** Fallback for loading the fonts and icons if no route ever renders (e.g. the API is down). */
const FIRST_SCREEN_FALLBACK_MS = 3000;

export function App({ history }: { history?: RouterHistory }) {
  // One client and router per mounted app, so tests never share cache.
  const [queryClient] = useState(createQueryClient);
  const [router] = useState(() => createAppRouter(queryClient, history));
  // Tab icons + manifest: once the first screen's paint is reported (app/deferredHead.ts).
  useEffect(() => {
    let active = true;
    const unsubscribe = router.subscribe("onRendered", () => {
      void afterFirstPaint().then(() => {
        // An unmounted app (or a torn-down test DOM) has nothing to declare into.
        if (active) declareDeferredHead();
      });
    });
    const timer = setTimeout(declareDeferredHead, FIRST_SCREEN_FALLBACK_MS);
    return () => {
      active = false;
      unsubscribe();
      clearTimeout(timer);
    };
  }, [router]);
  useEffect(() => {
    if (loadFontsIfCached()) return;
    const unsubscribe = router.subscribe("onRendered", loadFontsAfterFirstScreen);
    const timer = setTimeout(loadFontsAfterFirstScreen, FIRST_SCREEN_FALLBACK_MS);
    return () => {
      unsubscribe();
      clearTimeout(timer);
    };
  }, [router]);
  return (
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
    </QueryClientProvider>
  );
}
