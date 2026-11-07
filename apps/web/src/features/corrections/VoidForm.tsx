import { useEffect, useId, useRef, useState } from "react";
import { fieldMessage, messageFor } from "../../lib/errors";
import { useFormAction } from "./useFormAction";
import "./messages";

const MAX_REASON = 500;

/**
 * Inline "Hủy … — lý do" form (US6). The reason is required here as a UX shortcut; the server
 * enforces it (422), as it does the closed-day rule (403 day.locked for managers).
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
  // One key for the life of this form: double click or retry = the same void.
  const [idempotencyKey] = useState(() => crypto.randomUUID());
  const [reason, setReason] = useState("");
  const [touched, setTouched] = useState(false);
  const action = useFormAction({
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
      className="mt-2 grid gap-3 rounded-md border border-danger/50 bg-surface p-3"
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
        className="grid gap-2"
      >
        <label htmlFor={reasonId} className="field">
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
          className="input"
        />
        {reasonProblem && (
          <p id={`${reasonId}-error`} className="field-error">
            {reasonProblem}
          </p>
        )}
        {action.error != null && !fieldMessage(action.error, "reason") && (
          <p role="alert" className="alert">
            {messageFor(action.error)}
          </p>
        )}
        <div className="button-row">
          <button
            type="submit"
            disabled={action.pending}
            className="btn btn-danger"
          >
            {action.pending ? "Đang hủy…" : confirmLabel}
          </button>
          <button
            type="button"
            onClick={onCancel}
            className="btn btn-secondary"
          >
            Thôi
          </button>
        </div>
      </form>
    </section>
  );
}
