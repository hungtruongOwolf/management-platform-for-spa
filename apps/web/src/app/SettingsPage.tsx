import { useSuspenseQuery } from "@tanstack/react-query";
import { useState } from "react";
import { meQuery } from "../features/auth/session";
import { initialOf } from "./AppShell";
import { Icon, type IconName } from "./Icon";
import { applyTheme, readTheme, type Theme } from "./prefs";

const THEMES: [Theme, string, IconName][] = [
  ["system", "Theo thiết bị", "monitor"],
  ["light", "Sáng", "sun"],
  ["dark", "Tối", "moon"],
];

/** /cai-dat — appearance on this device, and who is signed in. Staff and roles come with spec #4. */
export function SettingsPage() {
  const { data: me } = useSuspenseQuery(meQuery);
  const [theme, setTheme] = useState<Theme>(readTheme);
  return (
    <section className="max-w-3xl space-y-6">
      <div>
        <p className="eyebrow">Cài đặt</p>
        <h1 className="page-title">Giao diện &amp; tài khoản</h1>
      </div>

      <section aria-labelledby="theme-title" className="card space-y-3 p-5">
        <h2 id="theme-title" className="font-semibold">
          Giao diện
        </h2>
        <p className="text-sm text-text-muted">Chỉ áp dụng trên thiết bị này. Chế độ tối dễ nhìn hơn khi quầy thiếu sáng.</p>
        <div className="segmented" role="group" aria-label="Chế độ màu">
          {THEMES.map(([t, label, icon]) => (
            <button
              key={t}
              type="button"
              aria-pressed={theme === t}
              onClick={() => {
                applyTheme(t);
                setTheme(t);
              }}
            >
              <Icon name={icon} className="size-[18px]" />
              {label}
            </button>
          ))}
        </div>
      </section>

      <section aria-labelledby="account-title" className="card p-5">
        <h2 id="account-title" className="mb-4 font-semibold">
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
              <dt className="w-24 text-text-muted">Spa</dt>
              <dd>{me?.active_tenant?.name ?? "Chưa chọn spa"}</dd>
            </div>
          </dl>
        </div>
        <p className="mt-4 text-sm text-text-muted">Đổi mật khẩu, nhân viên và vai trò sẽ có ở đây trong đợt sau.</p>
      </section>
    </section>
  );
}
