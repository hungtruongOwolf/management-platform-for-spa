import { useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { useState, type FormEvent } from "react";
import { ApiError, api } from "../../lib/api";
import { fieldMessage } from "../../lib/errors";
import { AuthFrame, bandTitle } from "./AuthFrame";
import { logout, parseMe, setAuthState, type Me } from "./session";

/** ADR-0016 password policy, as the server checks it (`identity::account::check_new_password`). */
export const MIN_PASSWORD = 10;

const FIELD_TEXT: Record<string, string> = {
  too_short: `Mật khẩu mới cần ít nhất ${String(MIN_PASSWORD)} ký tự.`,
  too_long: "Mật khẩu mới dài quá 128 ký tự.",
  same_as_current: "Mật khẩu mới phải khác mật khẩu hiện tại.",
  is_phone: "Không dùng số điện thoại làm mật khẩu.",
};

async function changePassword(current: string, next: string): Promise<Me> {
  return parseMe(await api("/auth/password", { method: "POST", body: { current_password: current, new_password: next } }));
}

/**
 * Current password + new password twice. On success every other device is signed out and this
 * one gets a fresh session (`onChanged` receives who is signed in). Used by the forced first
 * change and by "Đổi mật khẩu" in Cài đặt.
 */
export function ChangePasswordForm({ currentLabel, onChanged }: { currentLabel: string; onChanged: (me: Me) => void }) {
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const [again, setAgain] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState(false);
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (busy) return;
    setError(null);
    if (next.length < MIN_PASSWORD) {
      setError(FIELD_TEXT["too_short"] ?? null);
      return;
    }
    if (next !== again) {
      setError("Hai lần nhập mật khẩu mới không giống nhau.");
      return;
    }
    setBusy(true);
    try {
      const me = await changePassword(current, next);
      setDone(true);
      setCurrent("");
      setNext("");
      setAgain("");
      onChanged(me);
    } catch (err) {
      const code = fieldMessage(err, "new_password") && err instanceof ApiError ? err.fieldCode("new_password") : undefined;
      setError(
        code
          ? (FIELD_TEXT[code] ?? "Mật khẩu mới chưa hợp lệ.")
          : err instanceof ApiError && err.status === 401
            ? "Mật khẩu hiện tại không đúng."
            : err instanceof ApiError && err.status === 429
              ? "Thử nhiều lần quá. Chờ một lúc rồi thử lại."
              : "Chưa đổi được mật khẩu. Vui lòng thử lại.",
      );
    } finally {
      setBusy(false);
    }
  };
  return (
    <form className="grid gap-4" onSubmit={(e) => void submit(e)} noValidate>
      <div className="field">
        <label htmlFor="pw-current">{currentLabel}</label>
        <input id="pw-current" type="password" autoComplete="current-password" className="input" value={current} onChange={(e) => { setCurrent(e.target.value); }} required />
      </div>
      <div className="field">
        <label htmlFor="pw-new">Mật khẩu mới</label>
        <input id="pw-new" type="password" autoComplete="new-password" className="input" value={next} onChange={(e) => { setNext(e.target.value); }} aria-describedby="pw-rule" required />
        <p id="pw-rule" className="text-sm text-text-muted">
          Ít nhất {MIN_PASSWORD} ký tự, khác mật khẩu cũ, không dùng số điện thoại. Một câu ngắn dễ nhớ là mật khẩu tốt.
        </p>
      </div>
      <div className="field">
        <label htmlFor="pw-again">Nhập lại mật khẩu mới</label>
        <input id="pw-again" type="password" autoComplete="new-password" className="input" value={again} onChange={(e) => { setAgain(e.target.value); }} required />
      </div>
      {error && (
        <p role="alert" className="alert">
          {error}
        </p>
      )}
      {done && !error && (
        <p role="status" className="note">
          Đã đổi mật khẩu. Các thiết bị khác đã được đăng xuất.
        </p>
      )}
      <button type="submit" className="btn btn-primary" aria-busy={busy || undefined} disabled={!current || !next || !again}>
        Đổi mật khẩu
      </button>
    </form>
  );
}

/**
 * /doi-mat-khau: the first sign-in with a temporary password (ADR-0016). Nothing else opens until
 * the person sets their own; "Đăng xuất" is the way out.
 */
export function PasswordChangePage() {
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  return (
    <AuthFrame brand={<p className={bandTitle}>Viên Phúc Y Đường</p>}>
      <div className="grid gap-4 p-6 sm:p-10">
        <div>
          <h1 className="font-brand text-2xl text-primary">Đặt mật khẩu mới</h1>
          <p className="text-sm text-text-muted">Bạn đang dùng mật khẩu tạm chủ spa đưa. Đặt mật khẩu riêng để tiếp tục.</p>
        </div>
        <ChangePasswordForm
          currentLabel="Mật khẩu tạm"
          onChanged={(me) => {
            setAuthState(queryClient, { state: "signed_in", me });
            void navigate({ to: "/" });
          }}
        />
        <button
          type="button"
          className="btn btn-ghost"
          onClick={() => {
            void logout(queryClient).then(() => navigate({ to: "/dang-nhap" }));
          }}
        >
          Đăng xuất
        </button>
      </div>
    </AuthFrame>
  );
}
