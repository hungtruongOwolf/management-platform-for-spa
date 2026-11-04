import { useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import { Link, Outlet, useNavigate } from "@tanstack/react-router";
import { can, logout, meQuery } from "../features/auth/session";

const navLink =
  "inline-flex min-h-11 items-center rounded-[10px] px-3 font-medium text-text-muted hover:bg-surface-2 [&.active]:bg-primary-soft [&.active]:text-primary";

/** Signed-in frame: header, navigation (shown by permission — UX only), and the current screen. */
export function AppShell() {
  const { data: me } = useSuspenseQuery(meQuery);
  const queryClient = useQueryClient();
  const navigate = useNavigate();

  // "Khóa nhanh": leave the counter safely in one tap.
  const lock = async () => {
    await logout(queryClient);
    await navigate({ to: "/dang-nhap" });
  };

  return (
    <div className="min-h-dvh bg-bg">
      <header className="flex flex-wrap items-center gap-3 border-b border-border bg-surface px-4 py-2">
        <span className="font-brand text-xl text-primary">
          Viên Phúc Y Đường
        </span>
        <nav aria-label="Chính" className="flex gap-1">
          <Link to="/" className={navLink} activeOptions={{ exact: true }}>
            Trang chủ
          </Link>
          {can(me, "customer.read") && (
            <Link to="/khach-hang" className={navLink}>
              Khách hàng
            </Link>
          )}
          {can(me, "catalog.read") && (
            <Link to="/bang-gia" className={navLink}>
              Bảng giá
            </Link>
          )}
        </nav>
        <span className="flex-1" />
        <span className="text-sm text-text-muted">
          {me?.user.full_name} · {me?.active_tenant?.name ?? "Chưa chọn spa"}
        </span>
        <button
          type="button"
          onClick={() => void lock()}
          className="min-h-11 rounded-[10px] border border-border px-4 font-medium hover:bg-surface-2"
        >
          Khóa nhanh
        </button>
      </header>
      <main className="mx-auto max-w-5xl p-4">
        <Outlet />
      </main>
    </div>
  );
}

export function HomePage() {
  const { data: me } = useSuspenseQuery(meQuery);
  return (
    <>
      <h1 className="text-lg font-semibold">Xin chào, {me?.user.full_name}</h1>
      <p className="text-text-muted">
        Các màn hình nghiệp vụ sẽ xuất hiện ở đây.
      </p>
    </>
  );
}
