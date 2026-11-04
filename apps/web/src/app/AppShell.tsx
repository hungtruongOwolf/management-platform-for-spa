import { useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { logout, meQuery } from "../features/auth/session";

/** Signed-in frame. Feature screens (quầy, khách hàng, bảng giá…) mount inside it from S3 on. */
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
      <header className="flex items-center gap-3 border-b border-border bg-surface px-4 py-2">
        <span className="font-brand text-xl text-primary">Viên Phúc Y Đường</span>
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
        <h1 className="text-lg font-semibold">Xin chào, {me?.user.full_name}</h1>
        <p className="text-text-muted">Các màn hình nghiệp vụ sẽ xuất hiện ở đây.</p>
      </main>
    </div>
  );
}
