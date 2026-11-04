import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { useState } from "react";
import { ApiStatus } from "./ApiStatus";

export function App() {
  // One client per mounted app, so tests never share cache.
  const [queryClient] = useState(() => new QueryClient({ defaultOptions: { queries: { retry: false } } }));
  return (
    <QueryClientProvider client={queryClient}>
      <main className="mx-auto flex min-h-dvh max-w-xl flex-col items-center justify-center gap-4 p-4 text-center">
        <h1 className="font-brand text-3xl text-primary">Viên Phúc Y Đường</h1>
        <p className="text-text-muted">Nền tảng vận hành spa · đang xây dựng</p>
        <ApiStatus />
      </main>
    </QueryClientProvider>
  );
}
