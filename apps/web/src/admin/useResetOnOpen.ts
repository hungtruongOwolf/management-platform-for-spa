import { useState } from "react";

/**
 * Runs `reset` in the render where `open` turns true, so a sheet closed with "Hủy" never reopens
 * with the last attempt's text or error. Adjusting state during render (not in an effect) means
 * the stale values are never painted.
 */
export function useResetOnOpen(open: boolean, reset: () => void): void {
  const [wasOpen, setWasOpen] = useState(open);
  if (open !== wasOpen) {
    setWasOpen(open);
    if (open) reset();
  }
}
