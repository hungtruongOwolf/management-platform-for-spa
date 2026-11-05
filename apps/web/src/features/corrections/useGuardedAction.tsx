import { useMutation } from "@tanstack/react-query";
import { type ReactNode, useState } from "react";
import { ApiError } from "../../lib/api";
import { ReauthDialog } from "../auth/ReauthDialog";

/**
 * A mutation that may need a recent password confirmation (🔒). When the server answers
 * `401 auth.reauth_required`, the re-auth dialog appears; once confirmed, the same request is
 * sent again (same idempotency key, so it can never run twice).
 */
export function useGuardedAction<T>({
  run,
  onSuccess,
}: {
  run: () => Promise<T>;
  onSuccess: (result: T) => void;
}): {
  submit: () => void;
  pending: boolean;
  /** The error to show, if any (never the re-auth request itself). */
  error: unknown;
  reauth: ReactNode;
} {
  const [needsReauth, setNeedsReauth] = useState(false);
  const mutation = useMutation({
    mutationFn: run,
    onSuccess,
    onError: (error) => {
      if (error instanceof ApiError && error.code === "auth.reauth_required") {
        setNeedsReauth(true);
      }
    },
  });
  const isReauth =
    mutation.error instanceof ApiError &&
    mutation.error.code === "auth.reauth_required";
  return {
    submit: () => {
      mutation.mutate();
    },
    pending: mutation.isPending,
    error: mutation.isError && !isReauth ? mutation.error : null,
    reauth: needsReauth ? (
      <ReauthDialog
        onConfirmed={() => {
          setNeedsReauth(false);
          mutation.mutate();
        }}
        onCancel={() => {
          setNeedsReauth(false);
          mutation.reset();
        }}
      />
    ) : null,
  };
}
