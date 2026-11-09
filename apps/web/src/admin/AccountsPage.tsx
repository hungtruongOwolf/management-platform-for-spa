import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useState, type SyntheticEvent } from "react";
import { BottomSheet } from "../components/BottomSheet";
import { adminApi, adminDateTime, adminKeys, type AccountAction, type AccountRow } from "./api";
import { adminMessage } from "./messages";
import { ReasonSheet } from "./ReasonSheet";

type Pending = { account: AccountRow; action: AccountAction | "reset-password" };

const LABELS: Record<Pending["action"], { title: string; confirm: string; description: string; danger: boolean }> = {
  lock: {
    title: "Khóa tài khoản",
    confirm: "Khóa",
    description: "Người này không đăng nhập được ở mọi spa, mọi phiên bị đăng xuất.",
    danger: true,
  },
  unlock: { title: "Mở khóa tài khoản", confirm: "Mở khóa", description: "Đăng nhập lại được; xóa khóa tạm do nhập sai.", danger: false },
  "reset-password": {
    title: "Đặt lại mật khẩu",
    confirm: "Đặt lại",
    description: "Mật khẩu tạm mới (hiện một lần), mọi phiên bị đăng xuất, lần đăng nhập sau phải đổi mật khẩu.",
    danger: true,
  },
  "reset-mfa": {
    title: "Đặt lại xác thực hai bước",
    confirm: "Đặt lại",
    description: "Xóa ứng dụng xác thực và mã khôi phục; lần đăng nhập sau phải thiết lập lại.",
    danger: true,
  },
  "revoke-sessions": {
    title: "Đăng xuất mọi thiết bị",
    confirm: "Đăng xuất",
    description: "Mọi phiên đang mở của tài khoản này kết thúc.",
    danger: false,
  },
};

/** "Tài khoản" (spec US8): logins across spas — never a hash, a secret or business data. */
export function AccountsPage() {
  const queryClient = useQueryClient();
  const [draft, setDraft] = useState("");
  const [q, setQ] = useState("");
  const accounts = useQuery({
    queryKey: adminKeys.accounts(q),
    queryFn: () => adminApi.accounts(q),
    enabled: q.length >= 3,
  });
  const [pending, setPending] = useState<Pending | null>(null);
  const [password, setPassword] = useState<string | null>(null);

  const search = (e: SyntheticEvent<HTMLFormElement>) => {
    e.preventDefault();
    setQ(draft.trim());
  };
  const run = async (reason: string) => {
    if (!pending) return;
    if (pending.action === "reset-password") {
      const reset = await adminApi.resetPassword(pending.account.id, reason);
      setPassword(reset.temporary_password);
    } else {
      await adminApi.accountAction(pending.account.id, pending.action, reason);
    }
    await queryClient.invalidateQueries({ queryKey: ["admin", "accounts"] });
  };
  const label = pending ? LABELS[pending.action] : null;

  return (
    <div className="grid content-start gap-4">
      <header className="page-header">
        <div className="page-header-main">
          <h1 className="page-title">Tài khoản</h1>
        </div>
      </header>
      <form role="search" className="search-row" onSubmit={search}>
        <label htmlFor="account-q" className="sr-only">
          Tìm theo số điện thoại hoặc tên
        </label>
        <input
          id="account-q"
          className="input"
          placeholder="Số điện thoại hoặc tên (ít nhất 3 ký tự)"
          value={draft}
          onChange={(e) => {
            setDraft(e.target.value);
          }}
        />
        <button type="submit" className="btn btn-primary">
          Tìm
        </button>
      </form>
      <section className="panel" aria-label="Kết quả">
        {q.length < 3 ? (
          <p className="empty-note">Nhập số điện thoại hoặc tên để tìm tài khoản.</p>
        ) : accounts.isPending ? (
          <div role="status" aria-label="Đang tải" className="skeleton h-24" />
        ) : accounts.isError ? (
          <p role="alert" className="alert">
            {adminMessage(accounts.error)}
          </p>
        ) : accounts.data.length === 0 ? (
          <p className="empty-note">Không có tài khoản nào khớp.</p>
        ) : (
          <ul className="grid gap-3">
            {accounts.data.map((a) => (
              <li key={a.id} className="grid gap-2 border-b border-border pb-3">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="font-medium">{a.full_name}</span>
                  <span className="tabular text-sm">{a.phone}</span>
                  {a.status === "disabled" && <span className="badge badge-danger">Đã khóa</span>}
                  {a.locked_until && <span className="badge badge-warn">Khóa tạm tới {adminDateTime(a.locked_until)}</span>}
                  {a.mfa_enabled && <span className="badge badge-info">Có xác thực hai bước</span>}
                  {a.must_change_password && <span className="badge badge-muted">Đang dùng mật khẩu tạm</span>}
                </div>
                <p className="text-sm text-text-muted">
                  {a.memberships.length === 0
                    ? "Không thuộc spa nào"
                    : a.memberships
                        .map((m) => `${m.tenant_name} · ${m.is_owner ? "chủ spa" : m.role_keys.join(", ")}${m.status === "active" ? "" : " (đã khóa ở spa)"}`)
                        .join(" — ")}
                </p>
                <div className="flex flex-wrap gap-2">
                  {(a.status === "disabled" || a.locked_until
                    ? (["unlock"] as const)
                    : (["lock", "reset-password", "revoke-sessions"] as const)
                  ).map((action) => (
                    <button
                      key={action}
                      type="button"
                      className={LABELS[action].danger ? "btn btn-danger-outline" : "btn btn-secondary"}
                      onClick={() => {
                        setPending({ account: a, action });
                      }}
                    >
                      {LABELS[action].title}
                    </button>
                  ))}
                  {a.mfa_enabled && a.status === "active" && (
                    <button
                      type="button"
                      className="btn btn-danger-outline"
                      onClick={() => {
                        setPending({ account: a, action: "reset-mfa" });
                      }}
                    >
                      {LABELS["reset-mfa"].title}
                    </button>
                  )}
                </div>
              </li>
            ))}
          </ul>
        )}
      </section>
      <ReasonSheet
        open={pending !== null && password === null}
        title={label && pending ? `${label.title}: ${pending.account.full_name}` : ""}
        description={label?.description ?? ""}
        confirm={label?.confirm ?? ""}
        danger={label?.danger ?? false}
        run={run}
        onClose={() => {
          if (password === null) setPending(null);
        }}
      />
      <BottomSheet
        open={password !== null}
        onClose={() => {
          setPassword(null);
          setPending(null);
        }}
        title="Đã đặt lại mật khẩu"
        description={pending ? `${pending.account.full_name} — ${pending.account.phone}` : undefined}
        footer={
          <button
            type="button"
            className="btn btn-primary"
            onClick={() => {
              setPassword(null);
              setPending(null);
            }}
          >
            Tôi đã lưu mật khẩu
          </button>
        }
      >
        <p className="callout">Chỉ hiện một lần. Đưa tận tay; người này phải đặt mật khẩu mới ở lần đăng nhập sau.</p>
        <p className="text-2xl font-mono tabular" aria-label="Mật khẩu tạm">
          {password}
        </p>
      </BottomSheet>
    </div>
  );
}
