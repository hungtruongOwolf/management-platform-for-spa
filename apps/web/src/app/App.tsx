import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider, type RouterHistory } from "@tanstack/react-router";
import { useEffect, useState } from "react";
import { loadFontsAfterFirstScreen, loadFontsIfCached } from "./fonts";
import { createAppRouter } from "./router";

/** Fallback for loading the fonts if no route ever renders (e.g. the API is down). */
const FONT_FALLBACK_MS = 3000;

export function App({ history }: { history?: RouterHistory }) {
  // One client and router per mounted app, so tests never share cache.
  const [queryClient] = useState(
    () => new QueryClient({ defaultOptions: { queries: { retry: false } } }),
  );
  const [router] = useState(() => createAppRouter(queryClient, history));
  useEffect(() => {
    if (loadFontsIfCached()) return;
    const unsubscribe = router.subscribe("onRendered", loadFontsAfterFirstScreen);
    const timer = setTimeout(loadFontsAfterFirstScreen, FONT_FALLBACK_MS);
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
