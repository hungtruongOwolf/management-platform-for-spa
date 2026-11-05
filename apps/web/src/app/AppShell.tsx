import { useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import { Link, Outlet, useNavigate } from "@tanstack/react-router";
import { can, logout, meQuery } from "../features/auth/session";
import { BrandMark } from "./BrandMark";

// Cream text on the maroon chrome; the current screen gets a gold underline.
const navLink =
  "inline-flex min-h-11 items-center rounded-[10px] px-3 font-medium text-chrome-text-muted hover:bg-[var(--chrome-hover)] hover:text-chrome-text [&.active]:bg-[var(--chrome-hover)] [&.active]:text-chrome-text [&.active]:shadow-[inset_0_-2px_0_var(--chrome-accent)]";

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
    <div className="min-h-dvh">
      {/* One row at every width (nav scrolls sideways instead of wrapping): the header height
          never changes when the web font swaps in, so the page below does not shift (CLS).
          The focus ring switches to the bright gold: the standard one is too dark on maroon. */}
      <header className="flex items-center gap-3 border-b-2 border-chrome-accent/70 bg-chrome bg-[linear-gradient(90deg,var(--chrome-bg),var(--chrome-bg-2))] px-4 py-2 text-chrome-text [--focus:var(--chrome-accent)]">
        <span className="flex shrink-0 items-center gap-2 whitespace-nowrap font-brand text-lg text-chrome-accent sm:text-xl">
          <BrandMark className="size-8 shrink-0" />
          {/* Phones: the mark alone, the name stays for screen readers. */}
          <span className="max-sm:sr-only">Viên Phúc Y Đường</span>
        </span>
        <nav
          aria-label="Chính"
          className="flex min-w-0 flex-1 gap-1 overflow-x-auto whitespace-nowrap"
        >
          <Link to="/" className={navLink} activeOptions={{ exact: true }}>
            Trang chủ
          </Link>
          {can(me, "visit.record") && (
            <Link to="/quay" className={navLink}>
              Quầy
            </Link>
          )}
          {can(me, "customer.read") && (
            <Link to="/khach-hang" className={navLink}>
              Khách hàng
            </Link>
          )}
          {can(me, "opening_balance.import") && (
            <Link to="/nhap-so-cu" className={navLink}>
              Nhập sổ cũ
            </Link>
          )}
          {can(me, "catalog.read") && (
            <Link to="/bang-gia" className={navLink}>
              Bảng giá
            </Link>
          )}
        </nav>
        <span className="hidden whitespace-nowrap text-sm text-chrome-text-muted lg:inline">
          {me?.user.full_name} · {me?.active_tenant?.name ?? "Chưa chọn spa"}
        </span>
        <button
          type="button"
          onClick={() => void lock()}
          className="min-h-11 shrink-0 whitespace-nowrap rounded-[10px] border border-chrome-text-muted/50 px-4 font-medium text-chrome-text hover:border-chrome-accent hover:bg-[var(--chrome-hover)]"
        >
          Khóa nhanh
        </button>
      </header>
      <main className="mx-auto max-w-6xl p-4 sm:px-6">
        <Outlet />
      </main>
    </div>
  );
}

const today = new Intl.DateTimeFormat("vi-VN", {
  weekday: "long",
  day: "numeric",
  month: "numeric",
  year: "numeric",
  timeZone: "Asia/Ho_Chi_Minh",
});

const tile =
  "group flex min-h-24 flex-col justify-between gap-2 rounded-[14px] border border-border bg-surface p-4 shadow-card hover:-translate-y-0.5 hover:border-accent hover:shadow-float";

export function HomePage() {
  const { data: me } = useSuspenseQuery(meQuery);
  return (
    <div className="space-y-6">
      <section className="relative overflow-hidden rounded-[14px] bg-chrome bg-[linear-gradient(120deg,var(--chrome-bg),var(--chrome-bg-2))] p-6 text-chrome-text shadow-float sm:p-8">
        <BrandMark className="pointer-events-none absolute -right-6 -bottom-10 size-48 text-chrome-accent opacity-20" />
        <p className="text-sm text-chrome-text-muted first-letter:uppercase">
          {today.format(new Date())}
        </p>
        <h1 className="mt-1 font-brand text-2xl text-chrome-text sm:text-3xl">
          Xin chào, {me?.user.full_name}
        </h1>
        <p className="mt-2 text-chrome-text-muted">
          {me?.active_tenant?.name ?? "Chưa chọn spa"} · Hồi sinh sức khỏe và sắc
          đẹp
        </p>
      </section>

      <nav aria-label="Lối tắt" className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
        {can(me, "visit.record") && (
          <Link to="/quay" className={tile}>
            <span className="font-semibold text-primary">Ghi lượt khách</span>
            <span className="text-sm text-text-muted">
              Tìm khách, trừ buổi, thu tiền
            </span>
          </Link>
        )}
        {can(me, "customer.read") && (
          <Link to="/khach-hang" className={tile}>
            <span className="font-semibold text-primary">Hồ sơ khách</span>
            <span className="text-sm text-text-muted">
              Liệu trình, lịch sử, lưu ý sức khỏe
            </span>
          </Link>
        )}
        {can(me, "opening_balance.import") && (
          <Link to="/nhap-so-cu" className={tile}>
            <span className="font-semibold text-primary">Chuyển sổ giấy</span>
            <span className="text-sm text-text-muted">
              Đưa liệu trình đang dùng từ sổ vào máy
            </span>
          </Link>
        )}
        {can(me, "catalog.read") && (
          <Link to="/bang-gia" className={tile}>
            <span className="font-semibold text-primary">Dịch vụ &amp; gói</span>
            <span className="text-sm text-text-muted">
              Menu, giá Nữ/Nam, gói liệu trình
            </span>
          </Link>
        )}
      </nav>
    </div>
  );
}
