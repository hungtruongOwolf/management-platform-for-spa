import { useState, type ReactNode, type SyntheticEvent } from "react";
import { BottomSheet } from "../components/BottomSheet";
import { adminFieldError, adminMessage } from "./messages";
import { StepUpCancelled, useStepUp } from "./StepUp";

/**
 * Every operator write asks why (the reason goes into both audit trails) and may ask for a
 * step-up. `run(reason)` performs the write; the sheet closes on success.
 */
export function ReasonSheet({
  open,
  title,
  description,
  confirm,
  danger = false,
  children,
  run,
  onClose,
}: {
  open: boolean;
  title: string;
  description: string;
  confirm: string;
  danger?: boolean;
  children?: ReactNode;
  run: (reason: string) => Promise<void>;
  onClose: () => void;
}) {
  const stepUp = useStepUp();
  const [reason, setReason] = useState("");
  const [error, setError] = useState<unknown>(null);
  const [busy, setBusy] = useState(false);

  const submit = async (e: SyntheticEvent<HTMLFormElement>) => {
    e.preventDefault();
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      await stepUp(() => run(reason.trim()));
      setReason("");
      onClose();
    } catch (err) {
      if (!(err instanceof StepUpCancelled)) setError(err);
    } finally {
      setBusy(false);
    }
  };

  const reasonError = adminFieldError(error, "reason");
  return (
    <BottomSheet
      open={open}
      onClose={onClose}
      title={title}
      description={description}
      footer={
        <div className="button-row">
          <button type="button" className="btn btn-ghost" onClick={onClose}>
            Hủy
          </button>
          <button
            type="submit"
            form="reason-form"
            className={danger ? "btn btn-danger" : "btn btn-primary"}
            aria-busy={busy || undefined}
          >
            {confirm}
          </button>
        </div>
      }
    >
      <form id="reason-form" noValidate className="grid gap-3" onSubmit={(e) => void submit(e)}>
        {children}
        <div className="field">
          <label htmlFor="reason-text">Lý do</label>
          <textarea
            id="reason-text"
            className="input"
            value={reason}
            onChange={(e) => {
              setReason(e.target.value);
            }}
            aria-invalid={reasonError ? true : undefined}
            aria-describedby={reasonError ? "reason-text-error" : undefined}
          />
          {reasonError && (
            <p id="reason-text-error" className="field-error">
              {reasonError}
            </p>
          )}
        </div>
        {error && !reasonError ? (
          <p role="alert" className="alert">
            {adminMessage(error)}
          </p>
        ) : null}
      </form>
    </BottomSheet>
  );
}
