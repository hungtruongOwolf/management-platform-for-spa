import { useQueryClient } from "@tanstack/react-query";
import { useState, type ReactNode, type SyntheticEvent } from "react";
import { AuthFrame } from "../features/auth/AuthFrame";
import { QrCode } from "../features/auth/QrCode";
import { adminApi, type OperatorEnrollment } from "./api";
import { adminMessage } from "./messages";
import { useAdminNav } from "./nav";

type Stage =
  | { step: "password" }
  | { step: "verify" }
  | { step: "enroll"; enrollment: OperatorEnrollment }
  | { step: "codes"; codes: string[] };

function Frame({ title, subtitle, children }: { title: string; subtitle: string; children: ReactNode }) {
  return (
    <AuthFrame brand={<p className="sr-only">Admin nền tảng</p>}>
      <section aria-labelledby="admin-login-title" className="grid content-start gap-4 p-6 sm:p-10">
        <div>
          <p className="eyebrow">Admin nền tảng</p>
          <h1 id="admin-login-title" className="font-brand text-2xl text-primary">
            {title}
          </h1>
          <p className="text-sm text-text-muted">{subtitle}</p>
        </div>
        {children}
      </section>
    </AuthFrame>
  );
}

function Alert({ message }: { message: string | null }) {
  return message ? (
    <p role="alert" className="alert">
      {message}
    </p>
  ) : null;
}

/** Operator sign-in: username + password, then always a TOTP code (enrolled at the first sign-in). */
export function AdminLogin() {
  const queryClient = useQueryClient();
  const { go } = useAdminNav();
  const [stage, setStage] = useState<Stage>({ step: "password" });
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [code, setCode] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const signedIn = async () => {
    await queryClient.invalidateQueries({ queryKey: ["admin"] });
    go("/admin");
  };

  const guarded = async (work: () => Promise<void>) => {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      await work();
    } catch (err) {
      setError(adminMessage(err));
    } finally {
      setBusy(false);
    }
  };

  const submitPassword = (e: SyntheticEvent<HTMLFormElement>) => {
    e.preventDefault();
    if (!username.trim() || !password) {
      setError("Nhập tên đăng nhập và mật khẩu.");
      return;
    }
    void guarded(async () => {
      try {
        const reply = await adminApi.login(username.trim(), password);
        if (reply.next === "mfa_verify") {
          setStage({ step: "verify" });
        } else {
          const enrollment = await adminApi.enrollStart();
          setStage({ step: "enroll", enrollment });
        }
      } finally {
        setPassword("");
      }
    });
  };

  const submitCode = (e: SyntheticEvent<HTMLFormElement>) => {
    e.preventDefault();
    if (!code.trim()) {
      setError("Nhập mã trong ứng dụng xác thực.");
      return;
    }
    void guarded(async () => {
      try {
        if (stage.step === "enroll") {
          const done = await adminApi.enrollConfirm(code.trim());
          setStage({ step: "codes", codes: done.recovery_codes });
        } else {
          await adminApi.verify(code.trim());
          await signedIn();
        }
      } finally {
        setCode("");
      }
    });
  };

  if (stage.step === "codes") {
    return (
      <Frame title="Mã khôi phục" subtitle="Lưu các mã này ở nơi an toàn. Chúng chỉ hiện một lần; mỗi mã dùng được một lần khi mất điện thoại.">
        <ul aria-label="Mã khôi phục" className="grid grid-cols-2 gap-2 font-mono text-sm">
          {stage.codes.map((c) => (
            <li key={c} className="tabular">
              {c}
            </li>
          ))}
        </ul>
        <button type="button" className="btn btn-primary btn-lg btn-block" onClick={() => void signedIn()}>
          Tôi đã lưu, vào trang quản trị
        </button>
      </Frame>
    );
  }

  if (stage.step === "password") {
    return (
      <Frame title="Đăng nhập" subtitle="Dành cho người vận hành nền tảng. Luôn cần mã xác thực hai bước.">
        <form noValidate className="grid gap-4" onSubmit={submitPassword}>
          <div className="field">
            <label htmlFor="admin-username">Tên đăng nhập</label>
            <input
              id="admin-username"
              autoComplete="username"
              autoCapitalize="none"
              className="input"
              value={username}
              onChange={(e) => {
                setUsername(e.target.value);
              }}
            />
          </div>
          <div className="field">
            <label htmlFor="admin-password">Mật khẩu</label>
            <input
              id="admin-password"
              type="password"
              autoComplete="current-password"
              className="input"
              value={password}
              onChange={(e) => {
                setPassword(e.target.value);
              }}
            />
          </div>
          <Alert message={error} />
          <button type="submit" className="btn btn-primary btn-lg btn-block" aria-busy={busy || undefined}>
            Tiếp tục
          </button>
        </form>
      </Frame>
    );
  }

  const enrolling = stage.step === "enroll" ? stage.enrollment : null;
  return (
    <Frame
      title={enrolling ? "Thiết lập xác thực hai bước" : "Mã xác thực"}
      subtitle={
        enrolling
          ? "Quét mã bằng ứng dụng xác thực (Google Authenticator, Microsoft Authenticator…), rồi nhập mã 6 số."
          : "Nhập mã 6 số trong ứng dụng xác thực, hoặc một mã khôi phục."
      }
    >
      {enrolling && (
        <div className="grid justify-items-center gap-2">
          <QrCode value={enrolling.otpauth_uri} label="Mã QR để thêm vào ứng dụng xác thực" />
          <p className="text-sm text-text-muted">
            Hoặc nhập khóa: <span data-testid="admin-manual-key" className="font-mono tabular">{enrolling.manual_key}</span>
          </p>
        </div>
      )}
      <form noValidate className="grid gap-4" onSubmit={submitCode}>
        <div className="field">
          <label htmlFor="admin-code">Mã xác thực</label>
          <input
            id="admin-code"
            inputMode={enrolling ? "numeric" : "text"}
            autoComplete="one-time-code"
            className="input"
            value={code}
            onChange={(e) => {
              setCode(e.target.value);
            }}
          />
        </div>
        <Alert message={error} />
        <button type="submit" className="btn btn-primary btn-lg btn-block" aria-busy={busy || undefined}>
          Xác nhận
        </button>
        <button
          type="button"
          className="btn btn-ghost"
          onClick={() => {
            void adminApi.logout().catch(() => undefined);
            setStage({ step: "password" });
            setError(null);
          }}
        >
          Đăng nhập lại
        </button>
      </form>
    </Frame>
  );
}

