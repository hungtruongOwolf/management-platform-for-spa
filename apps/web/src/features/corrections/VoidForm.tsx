import { useEffect, useId, useRef, useState } from "react";
import { fieldMessage, messageFor } from "../../lib/errors";
import { useGuardedAction } from "./useGuardedAction";

const MAX_REASON = 500;

/**
 * Inline "Hủy … — lý do" form (US6). The reason is required here as a UX shortcut; the server
 * enforces it (422). An owner voiding a closed day is asked for the password, then it retries.
 */
export function VoidForm<T>({
  title,
  consequence,
  confirmLabel,
  run,
  onDone,
  onCancel,
}: {
  title: string;
  /** What will happen, e.g. "Buổi đã trừ được cộng lại; tiền đã thu được ghi bút toán đảo." */
  consequence: string;
  confirmLabel: string;
  run: (reason: string, idempotencyKey: string) => Promise<T>;
  onDone: (result: T) => void;
  onCancel: () => void;
}) {
  const titleId = useId();
  const reasonId = useId();
  const reasonRef = useRef<HTMLTextAreaElement>(null);
  // One key for the life of this form: double click, retry or re-auth = the same void.
  const [idempotencyKey] = useState(() => crypto.randomUUID());
  const [reason, setReason] = useState("");
  const [touched, setTouched] = useState(false);
  const action = useGuardedAction({
    run: () => run(reason.trim(), idempotencyKey),
    onSuccess: onDone,
  });

  useEffect(() => {
    reasonRef.current?.focus();
  }, []);

  const missing = reason.trim() === "";
  const reasonProblem =
    (touched && missing ? "Bắt buộc ghi lý do." : undefined) ??
    fieldMessage(action.error, "reason");

  return (
    <section
      role="dialog"
      aria-modal="false"
      aria-labelledby={titleId}
      onKeyDown={(e) => {
        if (e.key === "Escape") onCancel();
      }}
      className="mt-2 space-y-2 rounded-[10px] border border-danger/50 bg-surface p-3"
    >
      <h3 id={titleId} className="font-semibold">
        {title}
      </h3>
      <p className="text-sm text-text-muted">{consequence}</p>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          setTouched(true);
          if (!missing) action.submit();
        }}
        className="space-y-2"
      >
        <label htmlFor={reasonId} className="block text-sm font-medium">
          Lý do hủy
        </label>
        <textarea
          id={reasonId}
          ref={reasonRef}
          value={reason}
          maxLength={MAX_REASON}
          rows={2}
          aria-invalid={reasonProblem ? true : undefined}
          aria-describedby={reasonProblem ? `${reasonId}-error` : undefined}
          onChange={(e) => {
            setReason(e.target.value);
          }}
          className="w-full rounded-[6px] border border-border bg-surface px-3 py-2"
        />
        {reasonProblem && (
          <p id={`${reasonId}-error`} className="text-sm text-danger">
            {reasonProblem}
          </p>
        )}
        {action.error != null && !fieldMessage(action.error, "reason") && (
          <p role="alert" className="text-sm text-danger">
            {messageFor(action.error)}
          </p>
        )}
        <div className="flex flex-wrap gap-2">
          <button
            type="submit"
            disabled={action.pending}
            className="min-h-11 rounded-[10px] bg-danger px-4 font-semibold text-on-primary disabled:opacity-60"
          >
            {action.pending ? "Đang hủy…" : confirmLabel}
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
      {action.reauth}
    </section>
  );
}
