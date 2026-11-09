import { QueryClientProvider, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { ApiError } from "../lib/api";
import { createQueryClient } from "../lib/queryClient";
import { adminApi, adminKeys } from "./api";
import { AdminLogin } from "./AdminLogin";
import { AuditPage } from "./AuditPage";
import { adminMessage } from "./messages";
import { AdminLink, AdminNavProvider, matchAdminRoute, useAdminNav, type AdminRoute } from "./nav";
import { StepUpProvider } from "./StepUp";
import { TenantPage } from "./TenantPage";
import { TenantsPage } from "./TenantsPage";

/** The platform console (ADR-0018): its own query client, routes, session and frame. */
export function AdminApp() {
  const [queryClient] = useState(createQueryClient);
  return (
    <QueryClientProvider client={queryClient}>
      <AdminNavProvider>
        <StepUpProvider>
          <AdminRoot />
        </StepUpProvider>
      </AdminNavProvider>
    </QueryClientProvider>
  );
}

function signedOut(error: unknown): boolean {
  return error instanceof ApiError && error.status === 401;
}

function AdminRoot() {
  const { path, go } = useAdminNav();
  const route = matchAdminRoute(path);
  const me = useQuery({
    queryKey: adminKeys.me,
    queryFn: adminApi.me,
    retry: false,
    enabled: route.page !== "login",
  });

  useEffect(() => {
    if (route.page !== "login" && me.isError && signedOut(me.error)) go("/admin/dang-nhap");
  }, [route.page, me.isError, me.error, go]);

  useEffect(() => {
    document.title = "Admin nền tảng · Viên Phúc Y Đường";
  }, []);

  if (route.page === "login") return <AdminLogin />;
  if (me.isPending || (me.isError && signedOut(me.error))) {
    return (
      <div role="status" aria-label="Đang tải" className="mx-auto grid max-w-6xl gap-4 p-8">
        <div className="skeleton h-8 w-64" />
        <div className="skeleton h-64" />
      </div>
    );
  }
  if (me.isError) {
    return (
      <main className="mx-auto grid max-w-xl gap-3 p-8">
        <p role="alert" className="alert">
          {adminMessage(me.error)}
        </p>
        <button type="button" className="btn btn-secondary" onClick={() => void me.refetch()}>
          Thử lại
        </button>
      </main>
    );
  }
  return (
    <AdminShell name={me.data.operator.full_name} route={route}>
      <Page route={route} />
    </AdminShell>
  );
}

function Page({ route }: { route: AdminRoute }) {
  switch (route.page) {
    case "tenants":
      return <TenantsPage />;
    case "tenant":
      return <TenantPage id={route.id} />;
    case "audit":
      return <AuditPage />;
    default:
      return (
        <div className="grid gap-3">
          <h1 className="page-title">Không tìm thấy trang</h1>
          <AdminLink to="/admin" className="link">
            Về danh sách spa
          </AdminLink>
        </div>
      );
  }
}

const NAV: { to: string; label: string; pages: AdminRoute["page"][] }[] = [
  { to: "/admin", label: "Spa", pages: ["tenants", "tenant"] },
  { to: "/admin/nhat-ky", label: "Nhật ký", pages: ["audit"] },
];

function AdminShell({ name, route, children }: { name: string; route: AdminRoute; children: React.ReactNode }) {
  const queryClient = useQueryClient();
  const { go } = useAdminNav();
  const signOut = async () => {
    await adminApi.logout().catch(() => undefined);
    queryClient.clear();
    go("/admin/dang-nhap");
  };
  return (
    <div className="min-h-dvh">
      <header className="chrome flex flex-wrap items-center gap-4 px-4 py-3 sm:px-8">
        <span className="font-brand text-lg">Admin nền tảng</span>
        <nav aria-label="Admin nền tảng" className="flex gap-1">
          {NAV.map((item) => (
            <AdminLink
              key={item.to}
              to={item.to}
              className="btn btn-ghost"
              current={item.pages.includes(route.page)}
            >
              {item.label}
            </AdminLink>
          ))}
        </nav>
        <span className="ml-auto text-sm">{name}</span>
        <button type="button" className="btn btn-secondary" onClick={() => void signOut()}>
          Đăng xuất
        </button>
      </header>
      <main className="mx-auto grid w-full max-w-6xl content-start gap-4 p-4 sm:p-8">{children}</main>
    </div>
  );
}
