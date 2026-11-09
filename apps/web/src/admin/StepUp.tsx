import { createContext, useCallback, useContext, useRef, useState, type ReactNode, type SyntheticEvent } from "react";
import { BottomSheet } from "../components/BottomSheet";
import { ApiError } from "../lib/api";
import { adminApi } from "./api";
import { adminMessage } from "./messages";
import { useResetOnOpen } from "./useResetOnOpen";

type Run = <T>(write: () => Promise<T>) => Promise<T>;

const StepUpContext = createContext<Run | null>(null);

function isStepUp(error: unknown): boolean {
  return error instanceof ApiError && error.status === 403 && error.code === "platform.step_up_required";
}

export class StepUpCancelled extends Error {
  constructor() {
    super("step-up cancelled");
    this.name = "StepUpCancelled";
  }
}

/**
 * Every console write may answer `403 platform.step_up_required` (ADR-0018 D1: password + code
 * within 5 minutes). `useStepUp()(write)` runs the write; on that answer it asks for the password
 * and a code, confirms them (`POST /admin/auth/reauth`) and runs the same write once more. The
 * server decides; this only saves the operator from starting over.
 */
export function StepUpProvider({ children }: { children: ReactNode }) {
  const [open, setOpen] = useState(false);
  const pending = useRef<{ resolve: () => void; reject: (e: unknown) => void } | null>(null);

  const ask = useCallback(
    () =>
      new Promise<void>((resolve, reject) => {
        pending.current = { resolve, reject };
        setOpen(true);
      }),
    [],
  );

  const run = useCallback<Run>(
    async (write) => {
      try {
        return await write();
      } catch (error) {
        if (!isStepUp(error)) throw error;
        await ask();
        return await write();
      }
    },
    [ask],
  );

  const close = (outcome: "done" | "cancel") => {
    setOpen(false);
    const p = pending.current;
    pending.current = null;
    if (outcome === "done") p?.resolve();
    else p?.reject(new StepUpCancelled());
  };

  return (
    <StepUpContext.Provider value={run}>
      {children}
      <StepUpDialog
        open={open}
        onDone={() => {
          close("done");
        }}
        onCancel={() => {
          close("cancel");
        }}
      />
    </StepUpContext.Provider>
  );
}

export function useStepUp(): Run {
  const run = useContext(StepUpContext);
  if (!run) throw new Error("useStepUp outside StepUpProvider");
  return run;
}

function StepUpDialog({ open, onDone, onCancel }: { open: boolean; onDone: () => void; onCancel: () => void }) {
  const [password, setPassword] = useState("");
  const [code, setCode] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  useResetOnOpen(open, () => {
    setPassword("");
    setCode("");
    setError(null);
  });

  const submit = async (e: SyntheticEvent<HTMLFormElement>) => {
    e.preventDefault();
    if (busy) return;
    if (!password || code.trim().length < 6) {
      setError("Nhập mật khẩu và mã 6 số trong ứng dụng xác thực.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await adminApi.reauth(password, code.trim());
      setPassword("");
      setCode("");
      onDone();
    } catch (err) {
      setError(adminMessage(err));
      setCode("");
    } finally {
      setBusy(false);
    }
  };

  return (
    <BottomSheet
      open={open}
      onClose={onCancel}
      title="Xác nhận lại"
      description="Thao tác này cần mật khẩu và mã xác thực trong 5 phút gần nhất."
      footer={
        <div className="button-row">
          <button type="button" className="btn btn-ghost" onClick={onCancel}>
            Hủy
          </button>
          <button type="submit" form="step-up-form" className="btn btn-primary" aria-busy={busy || undefined}>
            Xác nhận
          </button>
        </div>
      }
    >
      <form id="step-up-form" noValidate className="grid gap-4" onSubmit={(e) => void submit(e)}>
        <div className="field">
          <label htmlFor="step-up-password">Mật khẩu</label>
          <input
            id="step-up-password"
            type="password"
            autoComplete="current-password"
            className="input"
            value={password}
            onChange={(e) => {
              setPassword(e.target.value);
            }}
          />
        </div>
        <div className="field">
          <label htmlFor="step-up-code">Mã xác thực</label>
          <input
            id="step-up-code"
            inputMode="numeric"
            autoComplete="one-time-code"
            className="input"
            value={code}
            onChange={(e) => {
              setCode(e.target.value);
            }}
          />
        </div>
        {error && (
          <p role="alert" className="alert">
            {error}
          </p>
        )}
      </form>
    </BottomSheet>
  );
}
