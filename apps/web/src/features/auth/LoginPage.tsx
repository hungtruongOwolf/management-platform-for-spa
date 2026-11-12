import { useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { Suspense, lazy, useEffect, useRef, useState, type SyntheticEvent } from "react";
import { preloadScreen } from "../../app/screens";
import { useAfterFirstPaint } from "../../lib/afterFirstPaint";
import { NETWORK_MESSAGE, safeNextPath } from "../../lib/api";
import { AuthFrame, bandTitle } from "./AuthFrame";
import { login, setAuthState } from "./session";

// "Quên mật khẩu?" (ADR-0016, option A): its own chunk, loaded when asked for.
const ForgotPassword = lazy(() => import("./ForgotPassword").then((m) => ({ default: m.ForgotPassword })));

// The error texts are not in the startup chunk (lib/errors.ts): fetched after the first paint, so a
// refused sign-in still shows its message at once.
const loadErrors = () => import("../../lib/errors");

/** What a failed sign-in says. Texts that cannot even be fetched mean the network is down. */
async function failureText(error: unknown): Promise<string> {
  try {
    return (await loadErrors()).messageFor(error);
  } catch {
    return NETWORK_MESSAGE;
  }
}

// The login page is the public entry point, so it stays free of react-hook-form and zod: two
// fields do not need them, and they would sit on its critical path (frontend.md, perf budget).
// The server re-validates everything; these checks only spare a round trip.

interface Values {
  phone: string;
  password: string;
}

type Errors = Partial<Record<keyof Values, string>>;

/** Digits only; a pasted "+84 905 111 222" or "0905.111.222" becomes "0905111222". */
export function phoneDigits(text: string): string {
  return text.trim().replace(/^\+84/, "0").replace(/\D/g, "");
}

function validate({ phone, password }: Values): Errors {
  const errors: Errors = {};
  if (phone.trim().length < 9) errors.phone = "Nhập số điện thoại.";
  if (password.length < 1) errors.password = "Nhập mật khẩu.";
  return errors;
}

export function LoginPage({ next }: { next?: string | undefined }) {
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const [values, setValues] = useState<Values>({ phone: "", password: "" });
  const [errors, setErrors] = useState<Errors>({});
  const [submitted, setSubmitted] = useState(false);
  const [isSubmitting, setSubmitting] = useState(false);
  const [serverError, setServerError] = useState<string | null>(null);
  const [forgot, setForgot] = useState(false);
  const phoneInput = useRef<HTMLInputElement>(null);
  const passwordInput = useRef<HTMLInputElement>(null);
  const painted = useAfterFirstPaint();
  useEffect(() => {
    if (painted) void loadErrors();
  }, [painted]);

  const change = (field: keyof Values, value: string) => {
    const nextValues = { ...values, [field]: value };
    setValues(nextValues);
    // After the first submit, errors follow the input as it is corrected.
    if (submitted) setErrors(validate(nextValues));
  };

  const onSubmit = async (e: SyntheticEvent<HTMLFormElement>) => {
    e.preventDefault();
    if (isSubmitting) return;
    setSubmitted(true);
    const found = validate(values);
    setErrors(found);
    if (found.phone || found.password) {
      // Like react-hook-form elsewhere: focus the first invalid field.
      (found.phone ? phoneInput : passwordInput).current?.focus();
      return;
    }

    setServerError(null);
    setSubmitting(true);
    // The screen sign-in leads to is a lazy chunk (app/screens.ts): it downloads during the request.
    preloadScreen(safeNextPath(next));
    try {
      const auth = await login(values.phone.trim(), values.password);
      setAuthState(queryClient, auth);
      if (auth.state === "signed_in") {
        await navigate({ to: safeNextPath(next) });
      } else if (auth.state === "password_change") {
        await navigate({ to: "/doi-mat-khau" });
      } else {
        // Second factor still owed (owners): the server refuses this session everywhere else.
        await navigate({ to: "/xac-thuc", search: next ? { next } : {} });
      }
    } catch (error) {
      setServerError(await failureText(error));
      setValues((v) => ({ ...v, password: "" }));
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <AuthFrame
      brand={
        <h1 id="login-title" className={bandTitle}>
          Viên Phúc Y Đường
        </h1>
      }
    >
      <form
        onSubmit={(e) => void onSubmit(e)}
        noValidate
        className="space-y-4 p-6 sm:p-10"
        aria-labelledby="login-title"
      >
        <div className="mb-2">
          <p className="font-brand text-2xl text-primary">Đăng nhập</p>
          <p className="text-sm text-text-muted">Dùng số điện thoại và mật khẩu spa đã cấp.</p>
        </div>

        <div className="field">
          <label htmlFor="phone">
            Số điện thoại
          </label>
          <input
            ref={phoneInput}
            id="phone"
            name="phone"
            type="tel"
            inputMode="numeric"
            autoComplete="username"
            className="input"
            value={values.phone}
            onChange={(e) => {
              change("phone", phoneDigits(e.target.value));
            }}
            aria-invalid={errors.phone ? true : undefined}
            aria-describedby={errors.phone ? "phone-error" : undefined}
          />
          {errors.phone && (
            <p id="phone-error" className="field-error">
              {errors.phone}
            </p>
          )}
        </div>

        <div className="field">
          <label htmlFor="password">
            Mật khẩu
          </label>
          <input
            ref={passwordInput}
            id="password"
            name="password"
            type="password"
            autoComplete="current-password"
            className="input"
            value={values.password}
            onChange={(e) => {
              change("password", e.target.value);
            }}
            aria-invalid={errors.password ? true : undefined}
            aria-describedby={errors.password ? "password-error" : undefined}
          />
          {errors.password && (
            <p id="password-error" className="field-error">
              {errors.password}
            </p>
          )}
        </div>

        {serverError && (
          <p role="alert" className="alert">
            {serverError}
          </p>
        )}

        <button
          type="submit"
          aria-busy={isSubmitting || undefined}
          className="btn btn-primary btn-lg btn-block"
        >
          Đăng nhập
        </button>
        {forgot ? (
          <Suspense fallback={null}>
            <ForgotPassword
              phone={values.phone}
              onClose={() => {
                setForgot(false);
              }}
            />
          </Suspense>
        ) : (
          <button
            type="button"
            className="btn btn-ghost btn-block"
            onClick={() => {
              setForgot(true);
            }}
          >
            Quên mật khẩu?
          </button>
        )}
      </form>
    </AuthFrame>
  );
}
