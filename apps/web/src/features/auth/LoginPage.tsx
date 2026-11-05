import { useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { useRef, useState, type SyntheticEvent } from "react";
import { safeNextPath } from "../../lib/api";
import { messageFor } from "../../lib/errors";
import { AuthFrame, bandTitle } from "./AuthFrame";
import { login, setAuthState } from "./session";

// The login page is the public entry point, so it stays free of react-hook-form and zod: two
// fields do not need them, and they would sit on its critical path (frontend.md, perf budget).
// The server re-validates everything; these checks only spare a round trip.

interface Values {
  phone: string;
  password: string;
}

type Errors = Partial<Record<keyof Values, string>>;

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
  const phoneInput = useRef<HTMLInputElement>(null);
  const passwordInput = useRef<HTMLInputElement>(null);

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
    try {
      const auth = await login(values.phone.trim(), values.password);
      setAuthState(queryClient, auth);
      if (auth.state === "signed_in") {
        await navigate({ to: safeNextPath(next) });
      } else {
        // Second factor still owed (owners): the server refuses this session everywhere else.
        await navigate({ to: "/xac-thuc", search: next ? { next } : {} });
      }
    } catch (error) {
      setServerError(messageFor(error));
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
        className="space-y-4 p-6"
        aria-labelledby="login-title"
      >
        <p className="text-center text-sm text-text-muted">Đăng nhập để vào hệ thống</p>

        <div className="space-y-1">
          <label htmlFor="phone" className="block text-sm font-semibold">
            Số điện thoại
          </label>
          <input
            ref={phoneInput}
            id="phone"
            name="phone"
            type="tel"
            inputMode="tel"
            autoComplete="username"
            className="min-h-11 w-full rounded-[6px] border border-border bg-surface px-3"
            value={values.phone}
            onChange={(e) => {
              change("phone", e.target.value);
            }}
            aria-invalid={errors.phone ? true : undefined}
            aria-describedby={errors.phone ? "phone-error" : undefined}
          />
          {errors.phone && (
            <p id="phone-error" className="text-sm text-danger">
              {errors.phone}
            </p>
          )}
        </div>

        <div className="space-y-1">
          <label htmlFor="password" className="block text-sm font-semibold">
            Mật khẩu
          </label>
          <input
            ref={passwordInput}
            id="password"
            name="password"
            type="password"
            autoComplete="current-password"
            className="min-h-11 w-full rounded-[6px] border border-border bg-surface px-3"
            value={values.password}
            onChange={(e) => {
              change("password", e.target.value);
            }}
            aria-invalid={errors.password ? true : undefined}
            aria-describedby={errors.password ? "password-error" : undefined}
          />
          {errors.password && (
            <p id="password-error" className="text-sm text-danger">
              {errors.password}
            </p>
          )}
        </div>

        {serverError && (
          <p role="alert" className="rounded-[10px] bg-danger-soft p-3 text-sm text-danger">
            {serverError}
          </p>
        )}

        <button
          type="submit"
          disabled={isSubmitting}
          className="min-h-12 w-full rounded-[10px] bg-primary font-semibold text-on-primary hover:bg-primary-hover disabled:opacity-60"
        >
          {isSubmitting ? "Đang đăng nhập…" : "Đăng nhập"}
        </button>
      </form>
    </AuthFrame>
  );
}
