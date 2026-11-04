import { useMutation } from "@tanstack/react-query";
import { useEffect, useId, useRef, useState } from "react";
import { ApiError } from "../../lib/api";
import { messageFor } from "../../lib/errors";
import { reauthenticate } from "./session";

const input =
  "min-h-11 w-full rounded-[6px] border border-border bg-surface px-3";

/**
 * 🔒 actions: asks for the password again (and the two-step code when the account has it — the
 * server says so with a 422 on `code`). Calls `onConfirmed` once the server accepted, so the
 * caller can retry its action with the same idempotency key.
 */
export function ReauthDialog({
  onConfirmed,
  onCancel,
}: {
  onConfirmed: () => void;
  onCancel: () => void;
}) {
  const titleId = useId();
  const passwordRef = useRef<HTMLInputElement>(null);
  const codeRef = useRef<HTMLInputElement>(null);
  const [password, setPassword] = useState("");
  const [code, setCode] = useState("");
  const [needsCode, setNeedsCode] = useState(false);

  useEffect(() => {
    passwordRef.current?.focus();
  }, []);

  const mutation = useMutation({
    mutationFn: () =>
      reauthenticate(password, needsCode ? code.trim() : undefined),
    onSuccess: onConfirmed,
    onError: (error) => {
      if (error instanceof ApiError && error.fieldCode("code") === "required") {
        setNeedsCode(true);
        setTimeout(() => codeRef.current?.focus(), 0);
      }
    },
  });

  const codeMissing =
    mutation.error instanceof ApiError &&
    mutation.error.fieldCode("code") === "required";

  return (
    <section
      role="dialog"
      aria-modal="false"
      aria-labelledby={titleId}
      onKeyDown={(e) => {
        if (e.key === "Escape") onCancel();
      }}
      className="mt-2 space-y-2 rounded-[10px] border border-warning-border bg-warning-bg p-3"
    >
      <h3 id={titleId} className="font-semibold text-warning-text">
        Xác nhận lại mật khẩu
      </h3>
      <p className="text-sm">
        Thao tác này cần xác nhận lại danh tính của bạn.
      </p>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          if (password !== "") mutation.mutate();
        }}
        className="space-y-2"
      >
        <label className="block">
          <span className="text-sm font-medium">Mật khẩu</span>
          <input
            ref={passwordRef}
            type="password"
            autoComplete="current-password"
            value={password}
            onChange={(e) => {
              setPassword(e.target.value);
            }}
            className={input}
          />
        </label>
        {needsCode && (
          <label className="block">
            <span className="text-sm font-medium">
              Mã xác thực hai bước (6 số hoặc mã khôi phục)
            </span>
            <input
              ref={codeRef}
              inputMode="numeric"
              autoComplete="one-time-code"
              value={code}
              onChange={(e) => {
                setCode(e.target.value);
              }}
              className={input}
            />
          </label>
        )}
        {mutation.isError && !codeMissing && (
          <p role="alert" className="text-sm text-danger">
            {messageFor(mutation.error)}
          </p>
        )}
        <div className="flex gap-2">
          <button
            type="submit"
            disabled={
              mutation.isPending ||
              password === "" ||
              (needsCode && code.trim() === "")
            }
            className="min-h-11 rounded-[10px] bg-primary px-4 font-semibold text-on-primary disabled:opacity-60"
          >
            {mutation.isPending ? "Đang xác nhận…" : "Xác nhận"}
          </button>
          <button
            type="button"
            onClick={onCancel}
            className="min-h-11 rounded-[10px] border border-border px-4"
          >
            Thôi
          </button>
        </div>
      </form>
    </section>
  );
}
