import { useState } from "react";
import { ApiError, api } from "../../lib/api";
import { phoneDigits } from "./LoginPage";

/**
 * "Quên mật khẩu?" (ADR-0016, option A): the phone → "Gửi yêu cầu". The spa's owner sees the
 * request and sets a temporary password. The answer is the same whether or not the phone has a
 * login, so this never says which phones are registered.
 */
export function ForgotPassword({ phone: initial, onClose }: { phone: string; onClose: () => void }) {
  const [phone, setPhone] = useState(initial);
  const [sent, setSent] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const submit = async (e: { preventDefault: () => void }) => {
    e.preventDefault();
    if (busy) return;
    if (phone.length < 9) {
      setError("Nhập số điện thoại.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await api("/auth/password-reset-requests", { method: "POST", body: { phone } });
      setSent(true);
    } catch (err) {
      setError(err instanceof ApiError && err.status === 429 ? "Gửi nhiều lần quá. Chờ một lúc rồi thử lại." : "Chưa gửi được. Vui lòng thử lại.");
    } finally {
      setBusy(false);
    }
  };
  if (sent)
    return (
      <div role="status" className="note grid gap-2">
        <p>Đã gửi. Nếu số này có tài khoản, chủ spa sẽ thấy yêu cầu và đặt lại mật khẩu cho bạn. Hỏi chủ spa để nhận mật khẩu tạm.</p>
        <button type="button" className="btn btn-ghost justify-self-start" onClick={onClose}>
          Quay lại đăng nhập
        </button>
      </div>
    );
  return (
    <section aria-labelledby="forgot-title" className="grid gap-3 border-t border-border pt-4">
      <h2 id="forgot-title" className="font-semibold">
        Quên mật khẩu
      </h2>
      <p className="text-sm text-text-muted">Gửi yêu cầu cho chủ spa. Chủ spa sẽ đặt mật khẩu tạm để bạn đăng nhập lại.</p>
      <div className="field">
        <label htmlFor="forgot-phone">Số điện thoại</label>
        <input
          id="forgot-phone"
          type="tel"
          inputMode="numeric"
          autoComplete="username"
          className="input"
          value={phone}
          onChange={(e) => {
            setPhone(phoneDigits(e.target.value));
          }}
          onKeyDown={(e) => {
            // Sits inside the login form: Enter sends this request, not the sign-in.
            if (e.key === "Enter") void submit(e);
          }}
        />
      </div>
      {error && (
        <p role="alert" className="alert">
          {error}
        </p>
      )}
      <div className="flex flex-wrap gap-2">
        <button type="button" className="btn btn-primary" aria-busy={busy || undefined} onClick={(e) => void submit(e)}>
          Gửi yêu cầu
        </button>
        <button type="button" className="btn btn-ghost" onClick={onClose}>
          Thôi
        </button>
      </div>
    </section>
  );
}
