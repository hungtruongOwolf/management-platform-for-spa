import { useMutation } from "@tanstack/react-query";

/**
 * The submit of a correction form (void, extend, cancel + refund): one mutation, with the error
 * left for the form to show. No password prompt: business actions never ask for re-auth (EM
 * decision 2026-11-05); the server still decides who may act.
 */
export function useFormAction<T>({
  run,
  onSuccess,
}: {
  run: () => Promise<T>;
  onSuccess: (result: T) => void;
}): {
  submit: () => void;
  pending: boolean;
  /** The error to show, if any. */
  error: unknown;
} {
  const mutation = useMutation({ mutationFn: run, onSuccess });
  return {
    submit: () => {
      mutation.mutate();
    },
    pending: mutation.isPending,
    error: mutation.isError ? mutation.error : null,
  };
}
