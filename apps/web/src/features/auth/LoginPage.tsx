import { zodResolver } from "@hookform/resolvers/zod";
import { useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { useState } from "react";
import { useForm } from "react-hook-form";
import { z } from "zod";
import { safeNextPath } from "../../lib/api";
import { messageFor } from "../../lib/errors";
import { login, setAuthState } from "./session";

const schema = z.object({
  phone: z.string().trim().min(9, "Nhập số điện thoại."),
  password: z.string().min(1, "Nhập mật khẩu."),
});
type FormValues = z.infer<typeof schema>;

export function LoginPage({ next }: { next?: string | undefined }) {
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const [serverError, setServerError] = useState<string | null>(null);
  const form = useForm<FormValues>({ resolver: zodResolver(schema), defaultValues: { phone: "", password: "" } });
  const { errors, isSubmitting } = form.formState;

  const onSubmit = form.handleSubmit(async ({ phone, password }) => {
    setServerError(null);
    try {
      const auth = await login(phone, password);
      setAuthState(queryClient, auth);
      if (auth.state === "signed_in") {
        await navigate({ to: safeNextPath(next) });
      } else {
        // Second factor still owed (owners): the server refuses this session everywhere else.
        await navigate({ to: "/xac-thuc", search: next ? { next } : {} });
      }
    } catch (error) {
      setServerError(messageFor(error));
      form.resetField("password");
    }
  });

  return (
    <main className="flex min-h-dvh items-center justify-center bg-bg p-4">
      <form
        onSubmit={(e) => void onSubmit(e)}
        noValidate
        className="w-full max-w-sm space-y-4 rounded-[14px] border border-border bg-surface p-6 shadow-[var(--shadow-1)]"
        aria-labelledby="login-title"
      >
        <div className="text-center">
          <h1 id="login-title" className="font-brand text-2xl text-primary">
            Viên Phúc Y Đường
          </h1>
          <p className="mt-1 text-sm text-text-muted">Đăng nhập để vào hệ thống</p>
        </div>

        <div className="space-y-1">
          <label htmlFor="phone" className="block text-sm font-semibold">
            Số điện thoại
          </label>
          <input
            id="phone"
            type="tel"
            inputMode="tel"
            autoComplete="username"
            className="min-h-11 w-full rounded-[6px] border border-border bg-surface px-3"
            aria-invalid={errors.phone ? true : undefined}
            aria-describedby={errors.phone ? "phone-error" : undefined}
            {...form.register("phone")}
          />
          {errors.phone && (
            <p id="phone-error" className="text-sm text-danger">
              {errors.phone.message}
            </p>
          )}
        </div>

        <div className="space-y-1">
          <label htmlFor="password" className="block text-sm font-semibold">
            Mật khẩu
          </label>
          <input
            id="password"
            type="password"
            autoComplete="current-password"
            className="min-h-11 w-full rounded-[6px] border border-border bg-surface px-3"
            aria-invalid={errors.password ? true : undefined}
            aria-describedby={errors.password ? "password-error" : undefined}
            {...form.register("password")}
          />
          {errors.password && (
            <p id="password-error" className="text-sm text-danger">
              {errors.password.message}
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
    </main>
  );
}
