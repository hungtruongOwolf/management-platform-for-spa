import { useQuery, useQueryClient } from "@tanstack/react-query";
import { getRouteApi, useNavigate } from "@tanstack/react-router";
import { useState, type SyntheticEvent, type ReactNode } from "react";
import { ApiError, safeNextPath } from "../../lib/api";
import { messageFor } from "../../lib/errors";
import { AuthFrame, bandTitle } from "./AuthFrame";
import { QrCode } from "./QrCode";
import { confirmMfaEnrollment, logout, setAuthState, startMfaEnrollment, verifyMfa, type Me, type MfaStep } from "./session";

const input = "input";
const primaryButton = "btn btn-primary btn-lg btn-block";
const linkButton = "btn btn-ghost";

function Card({ title, subtitle, children }: { title: string; subtitle: string; children: ReactNode }) {
  return (
    <AuthFrame brand={<p className={bandTitle}>Viên Phúc Y Đường</p>}>
      <section aria-labelledby="mfa-title" className="grid content-start gap-4 p-6 sm:p-10">
        <div className="text-center">
          <h1 id="mfa-title" className="font-brand text-xl text-primary">
            {title}
          </h1>
          <p className="mt-1 text-sm text-text-muted">{subtitle}</p>
        </div>
        {children}
      </section>
    </AuthFrame>
  );
}

function ErrorNote({ message }: { message: string | null }) {
  return message ? (
    <p role="alert" className="alert">
      {message}
    </p>
  ) : null;
}

/** The pending session expired or was ended: start over at the password step. */
function isSessionGone(error: unknown): boolean {
  return error instanceof ApiError && error.status === 401 && error.code === "auth.unauthenticated";
}

/** Second login step (UX only — the server refuses a pending session everywhere else). */
export function MfaPage({ step, next }: { step: MfaStep; next?: string | undefined }) {
  const queryClient = useQueryClient();
  const navigate = useNavigate();

  const finish = async (me: Me) => {
    setAuthState(queryClient, { state: "signed_in", me });
    await navigate({ to: safeNextPath(next) });
  };
  const restart = async () => {
    await logout(queryClient);
    await navigate({ to: "/dang-nhap" });
  };

  return step === "mfa_verify" ? (
    <VerifyStep onDone={finish} onRestart={restart} />
  ) : (
    <EnrollStep onDone={finish} onRestart={restart} />
  );
}

const mfaRouteApi = getRouteApi("/xac-thuc");

/**
 * The `/xac-thuc` route component, lazy-loaded by the router: only owners reach this step, so the
 * MFA screens and the QR encoder stay out of the main bundle (frontend.md, perf budget).
 */
export function MfaRoutePage() {
  const { next } = mfaRouteApi.useSearch();
  const { step } = mfaRouteApi.useRouteContext();
  return <MfaPage step={step} next={next} />;
}

interface StepProps {
  onDone: (me: Me) => Promise<void>;
  onRestart: () => Promise<void>;
}

function VerifyStep({ onDone, onRestart }: StepProps) {
  const [useRecovery, setUseRecovery] = useState(false);
  const [code, setCode] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const submit = async (e: SyntheticEvent) => {
    e.preventDefault();
    // The button stays focusable while busy (aria-busy): a second press does nothing.
    if (busy) return;
    if (!code.trim()) {
      setError(useRecovery ? "Nhập mã khôi phục." : "Nhập mã 6 số.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await onDone(await verifyMfa(code));
    } catch (err) {
      if (isSessionGone(err)) {
        await onRestart();
        return;
      }
      setError(messageFor(err));
      setCode("");
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card title="Xác thực hai bước" subtitle="Mở ứng dụng xác thực trên điện thoại và nhập mã đang hiển thị.">
      <form onSubmit={(e) => void submit(e)} noValidate className="space-y-4">
        <div className="field">
          <label htmlFor="mfa-code">
            {useRecovery ? "Mã khôi phục" : "Mã xác thực (6 số)"}
          </label>
          <input
            id="mfa-code"
            className={input}
            value={code}
            onChange={(e) => {
              setCode(e.target.value);
            }}
            autoComplete="one-time-code"
            inputMode={useRecovery ? "text" : "numeric"}
            maxLength={useRecovery ? 24 : 7}
            autoFocus
          />
        </div>
        <ErrorNote message={error} />
        <button type="submit" aria-busy={busy || undefined} className={primaryButton}>
          Xác nhận
        </button>
      </form>
      <div className="flex justify-between gap-2">
        <button
          type="button"
          className={linkButton}
          onClick={() => {
            setUseRecovery(!useRecovery);
            setCode("");
            setError(null);
          }}
        >
          {useRecovery ? "Dùng mã từ ứng dụng" : "Mất điện thoại? Dùng mã khôi phục"}
        </button>
        <button type="button" className={linkButton} onClick={() => void onRestart()}>
          Đăng xuất
        </button>
      </div>
    </Card>
  );
}

/** Groups the base32 key in fours so it can be typed into an authenticator by hand. */
function formatKey(key: string): string {
  return key.replace(/(.{4})/g, "$1 ").trim();
}

function EnrollStep({ onDone, onRestart }: StepProps) {
  // A query (not an effect) so StrictMode / re-renders never start enrollment twice.
  const enrollment = useQuery({
    queryKey: ["mfa", "enrollment"],
    queryFn: startMfaEnrollment,
    staleTime: Infinity,
    gcTime: 0,
    retry: false,
  });
  const [code, setCode] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState<{ me: Me; recoveryCodes: string[] } | null>(null);

  if (done) return <RecoveryCodes codes={done.recoveryCodes} onContinue={() => onDone(done.me)} />;

  if (enrollment.isError) {
    return (
      <Card title="Thiết lập xác thực hai bước" subtitle="">
        <ErrorNote
          message={
            isSessionGone(enrollment.error)
              ? "Phiên đăng nhập đã hết. Vui lòng đăng nhập lại."
              : messageFor(enrollment.error)
          }
        />
        <button type="button" className={primaryButton} onClick={() => void onRestart()}>
          Đăng nhập lại
        </button>
      </Card>
    );
  }

  const submit = async (e: SyntheticEvent) => {
    e.preventDefault();
    // The button stays focusable while busy (aria-busy): a second press does nothing.
    if (busy) return;
    if (!code.trim()) {
      setError("Nhập mã 6 số.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      setDone(await confirmMfaEnrollment(code));
    } catch (err) {
      if (isSessionGone(err)) {
        await onRestart();
        return;
      }
      setError(messageFor(err));
      setCode("");
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card
      title="Thiết lập xác thực hai bước"
      subtitle="Tài khoản chủ spa bắt buộc dùng ứng dụng xác thực (Google Authenticator, Microsoft Authenticator…)."
    >
      {enrollment.isPending ? (
        <p className="text-center text-sm text-text-muted" role="status">
          Đang tạo mã…
        </p>
      ) : (
        <>
          <ol className="list-decimal grid content-start gap-1 pl-5 text-sm">
            <li>Mở ứng dụng xác thực, chọn thêm tài khoản, quét mã QR.</li>
            <li>Nhập mã 6 số ứng dụng hiển thị.</li>
          </ol>
          <div className="flex justify-center">
            <QrCode value={enrollment.data.otpauth_uri} label="Mã QR để thêm tài khoản vào ứng dụng xác thực" />
          </div>
          <div className="grid content-start gap-1">
            <p className="text-sm font-semibold">Không quét được? Nhập khóa này:</p>
            <code data-testid="manual-key" className="block rounded-sm bg-surface-2 p-2 text-center font-mono text-sm break-all">
              {formatKey(enrollment.data.manual_key)}
            </code>
          </div>
          <form onSubmit={(e) => void submit(e)} noValidate className="space-y-4">
            <div className="field">
              <label htmlFor="enroll-code">
                Mã xác thực (6 số)
              </label>
              <input
                id="enroll-code"
                className={input}
                value={code}
                onChange={(e) => {
              setCode(e.target.value);
            }}
                autoComplete="one-time-code"
                inputMode="numeric"
                maxLength={7}
              />
            </div>
            <ErrorNote message={error} />
            <button type="submit" aria-busy={busy || undefined} className={primaryButton}>
              Bật xác thực hai bước
            </button>
          </form>
        </>
      )}
      <button type="button" className={linkButton} onClick={() => void onRestart()}>
        Đăng xuất
      </button>
    </Card>
  );
}

/** Shown once: the server keeps only hashes and never returns these codes again. */
function RecoveryCodes({ codes, onContinue }: { codes: string[]; onContinue: () => Promise<void> }) {
  const [saved, setSaved] = useState(false);
  const [copied, setCopied] = useState(false);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(codes.join("\n"));
      setCopied(true);
    } catch {
      setCopied(false);
    }
  };

  return (
    <Card
      title="Mã khôi phục"
      subtitle="Mỗi mã dùng được một lần khi bạn mất điện thoại. Chép hoặc in ra, cất nơi an toàn — mã chỉ hiện một lần này."
    >
      <ul aria-label="Mã khôi phục" className="grid grid-cols-2 gap-2 font-mono text-sm">
        {codes.map((c) => (
          <li key={c} className="rounded-sm bg-surface-2 p-2 text-center">
            {c}
          </li>
        ))}
      </ul>
      <button
        type="button"
        className="btn btn-secondary btn-block"
        onClick={() => void copy()}
      >
        {copied ? "Đã chép" : "Chép tất cả"}
      </button>
      <label className="choice">
        <input type="checkbox" checked={saved} onChange={(e) => {
            setSaved(e.target.checked);
          }} />
        Tôi đã lưu các mã khôi phục
      </label>
      <button type="button" disabled={!saved} className={primaryButton} onClick={() => void onContinue()}>
        Tiếp tục
      </button>
    </Card>
  );
}
