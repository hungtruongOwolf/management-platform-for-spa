import { useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import { ChangePasswordForm } from "../features/auth/PasswordChange";
import { meQuery, setAuthState } from "../features/auth/session";
import { PageHeader } from "./PageHeader";
import { ThemePicker } from "./ThemePicker";
import { initialOf, roleLabel } from "./UserMenu";

/** /cai-dat — appearance on this device, and who is signed in. Staff and roles come with spec #4. */
export function SettingsPage() {
  const { data: me } = useSuspenseQuery(meQuery);
  const queryClient = useQueryClient();
  return (
    <section className="grid max-w-3xl content-start gap-5">
      <PageHeader title="Giao diện & tài khoản" />

      <section aria-labelledby="theme-title" className="card grid content-start gap-3 p-4 sm:p-5">
        <h2 id="theme-title" className="panel-title">
          Giao diện
        </h2>
        <p className="text-sm text-text-muted">Chỉ áp dụng trên thiết bị này. Chế độ tối dễ nhìn hơn khi quầy thiếu sáng.</p>
        <ThemePicker />
      </section>

      <section aria-labelledby="account-title" className="card p-4 sm:p-5">
        <h2 id="account-title" className="panel-title mb-4">
          Tài khoản đang đăng nhập
        </h2>
        <div className="flex items-center gap-4">
          <span aria-hidden="true" className="avatar size-14 text-xl">
            {initialOf(me?.user.full_name ?? "")}
          </span>
          <dl className="grid gap-1 text-sm">
            <div className="flex gap-2">
              <dt className="w-24 text-text-muted">Họ tên</dt>
              <dd className="font-semibold">{me?.user.full_name}</dd>
            </div>
            <div className="flex gap-2">
              <dt className="w-24 text-text-muted">Điện thoại</dt>
              <dd className="tabular">{me?.user.phone}</dd>
            </div>
            <div className="flex gap-2">
              <dt className="w-24 text-text-muted">Vai trò</dt>
              <dd>{roleLabel(me ?? null)}</dd>
            </div>
            <div className="flex gap-2">
              <dt className="w-24 text-text-muted">Spa</dt>
              <dd>{me?.active_tenant?.name ?? "Chưa chọn spa"}</dd>
            </div>
          </dl>
        </div>
      </section>

      {/* ADR-0016: everyone changes their own password; other devices are signed out. */}
      <section aria-labelledby="password-title" className="card grid content-start gap-3 p-4 sm:p-5">
        <h2 id="password-title" className="panel-title">
          Đổi mật khẩu
        </h2>
        <ChangePasswordForm
          currentLabel="Mật khẩu hiện tại"
          onChanged={(fresh) => {
            setAuthState(queryClient, { state: "signed_in", me: fresh });
          }}
        />
      </section>
    </section>
  );
}
