import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useRef, useState } from "react";
import { useMediaQuery } from "../../components/useMediaQuery";
import { LIST_KEY, drawKey, spin } from "./api";
import { spinUnanswered } from "./messages";
import type { SpinOutcome } from "./types";
import type { WheelPhase } from "./Wheel";
import { LAND_MS, RETURN_MS, randomSector } from "./angles";

/** Margin after a transition's planned end before the timer settles it anyway (hidden tab, no event). */
const GRACE_MS = 400;

export interface Spin {
  phase: WheelPhase;
  sector: number;
  /** The wheel's resting angle, shared by every `Wheel` this spin drives. */
  angleRef: React.RefObject<number>;
  still: boolean;
  /** The result of this visit's last spin, shown only once the wheel stopped. */
  shownId: string | null;
  error: Error | null;
  busy: boolean;
  start: (prizeId: string) => void;
  /** The wheel's slow-down (or way back) ended. */
  settle: () => void;
}

/**
 * One program's spins (US4). Pressing "Quay" turns the wheel at once and asks the server, which
 * picks the winner and saves the result. The answer waits until the wheel has stopped on a random
 * sector (~4 s): only then does the program in the cache change, so the winner, the counts and the
 * lists all appear together (AC4.3). Reduced motion: the answer shows as soon as it arrives.
 *
 * A spin that got no clear answer (network, 5xx) keeps its Idempotency-Key, so pressing again
 * replays the first spin if it was saved after all (AC4.6); the wheel goes back where it was.
 */
export function useSpin(drawId: string, onFinished: () => void): Spin {
  const queryClient = useQueryClient();
  const still = useMediaQuery("(prefers-reduced-motion: reduce)");
  const [phase, setPhase] = useState<WheelPhase>("ready");
  const [sector, setSector] = useState(0);
  const [shownId, setShownId] = useState<string | null>(null);
  const angleRef = useRef(0);
  const retry = useRef<{ prizeId: string; key: string } | null>(null);
  const held = useRef<SpinOutcome | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const finished = useRef(onFinished);
  useEffect(() => {
    finished.current = onFinished;
  }, [onFinished]);

  const reveal = useCallback(() => {
    const outcome = held.current;
    if (!outcome) return;
    held.current = null;
    clearTimeout(timer.current);
    queryClient.setQueryData(drawKey(drawId), outcome.draw);
    void queryClient.invalidateQueries({ queryKey: LIST_KEY, exact: true });
    setShownId(outcome.result.id);
    setPhase("ready");
    if (outcome.draw.finished) finished.current();
  }, [drawId, queryClient]);

  const mutation = useMutation({
    mutationFn: (v: { prizeId: string; key: string }) => spin(drawId, v.prizeId, v.key),
    onSuccess: (outcome) => {
      retry.current = null;
      held.current = outcome;
      if (still) {
        reveal();
        return;
      }
      setSector(randomSector());
      setPhase("landing");
      timer.current = setTimeout(reveal, LAND_MS + GRACE_MS);
    },
    onError: (error) => {
      if (!spinUnanswered(error)) {
        retry.current = null;
        // The prize ran out or nobody fits any more (another tab spun): show the program as it is.
        void queryClient.invalidateQueries({ queryKey: drawKey(drawId), exact: true });
      }
      if (still) {
        setPhase("ready");
        return;
      }
      setPhase("returning");
      timer.current = setTimeout(() => {
        setPhase("ready");
      }, RETURN_MS + GRACE_MS);
    },
  });

  // Leaving mid-spin still puts the saved result in the cache.
  useEffect(
    () => () => {
      clearTimeout(timer.current);
      reveal();
    },
    [reveal],
  );

  const busy = phase !== "ready";
  const start = (prizeId: string) => {
    if (busy) return;
    const key = retry.current?.prizeId === prizeId ? retry.current.key : crypto.randomUUID();
    retry.current = { prizeId, key };
    setShownId(null);
    mutation.reset();
    setPhase("spinning");
    mutation.mutate({ prizeId, key });
  };

  const settle = () => {
    if (phase === "landing") reveal();
    else if (phase === "returning") {
      clearTimeout(timer.current);
      setPhase("ready");
    }
  };

  return {
    phase,
    sector,
    angleRef,
    still,
    shownId,
    error: phase === "ready" ? mutation.error : null,
    busy,
    start,
    settle,
  };
}
