import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { useMediaQuery } from "../../components/useMediaQuery";
import { drawKey, putDraw, spin } from "./api";
import { GENTLE, SPIN, randomSector, stopAngle } from "./angles";
import { spinUnanswered } from "./messages";
import type { SpinOutcome } from "./types";
import type { WheelTurn } from "./Wheel";

/** Margin after a transition's planned end before the timer ends it anyway (hidden tab, no event). */
const GRACE_MS = 400;

export interface Spin {
  /** The latest turn of the wheel (`null` before the first press of this visit). */
  turn: WheelTurn | null;
  /** The wheel's resting angle, shared by every `Wheel` this spin drives. */
  angleRef: React.RefObject<number>;
  /** The result of this visit's last spin, once the wheel has stopped on it. */
  shownId: string | null;
  /** Why the last spin did not land, once the wheel has stopped. */
  error: Error | null;
  /** From the press until the wheel has stopped and the server has answered. */
  busy: boolean;
  start: (prizeId: string) => void;
  /** The wheel's transition of turn `id` ended. */
  stopped: (id: number) => void;
}

interface Running {
  id: number;
  stopped: boolean;
  answer: { outcome: SpinOutcome } | { error: Error } | null;
}

/**
 * One program's spins (US4). Pressing "Quay" starts the wheel's turn at once (5 turns in 6 s to a
 * sector drawn at the press, `SPIN` in angles.ts; reduced motion: 3 gentler turns) and asks the
 * server, which picks the winner and saves the result. The spin lands only when **both** the wheel
 * has stopped and the answer is in: then the program in the cache changes, so the winner, the counts
 * and the lists all appear together (AC4.3), and `onRevealed` opens the winner's popup. An answer
 * later than the stop keeps the button busy until it arrives.
 *
 * A spin that got no clear answer (network, 5xx) keeps its Idempotency-Key, so pressing again
 * replays the first spin if it was saved after all (AC4.6); a clear refusal drops it.
 */
export function useSpin(drawId: string, onRevealed: (outcome: SpinOutcome) => void): Spin {
  const queryClient = useQueryClient();
  const gentle = useMediaQuery("(prefers-reduced-motion: reduce)");
  const [turn, setTurn] = useState<WheelTurn | null>(null);
  const [busy, setBusy] = useState(false);
  const [shownId, setShownId] = useState<string | null>(null);
  const [error, setError] = useState<Error | null>(null);
  const angleRef = useRef(0);
  const seq = useRef(0);
  const running = useRef<Running | null>(null);
  const retry = useRef<{ prizeId: string; key: string } | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const mounted = useRef(true);
  const revealed = useRef(onRevealed);
  useEffect(() => {
    revealed.current = onRevealed;
  }, [onRevealed]);

  /** Lands the running spin once the wheel has stopped and the server has answered. */
  const land = () => {
    const run = running.current;
    if (!run?.stopped || !run.answer) return;
    running.current = null;
    clearTimeout(timer.current);
    setBusy(false);
    if ("outcome" in run.answer) {
      const { outcome } = run.answer;
      putDraw(queryClient, outcome.draw);
      setShownId(outcome.result.id);
      revealed.current(outcome);
    } else {
      setError(run.answer.error);
    }
  };

  const stopped = (id: number) => {
    const run = running.current;
    if (run?.id !== id) return;
    run.stopped = true;
    land();
  };

  const mutation = useMutation({
    mutationFn: (v: { prizeId: string; key: string; id: number }) => spin(drawId, v.prizeId, v.key),
    onSuccess: (outcome, v) => {
      retry.current = null;
      const run = running.current;
      // Left the screen mid-spin, or a stale answer: the saved result still reaches the cache.
      if (!mounted.current || run?.id !== v.id) {
        putDraw(queryClient, outcome.draw);
        return;
      }
      run.answer = { outcome };
      land();
    },
    onError: (err, v) => {
      if (!spinUnanswered(err)) retry.current = null;
      // A refusal (the prize ran out, nobody fits any more: another tab spun) or a spin saved before
      // the connection dropped: show the program as it is now.
      void queryClient.invalidateQueries({ queryKey: drawKey(drawId), exact: true });
      const run = running.current;
      if (!mounted.current || run?.id !== v.id) return;
      run.answer = { error: err };
      land();
    },
  });

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      clearTimeout(timer.current);
      const answer = running.current?.answer;
      if (answer && "outcome" in answer) putDraw(queryClient, answer.outcome.draw);
    };
  }, [queryClient]);

  const start = (prizeId: string) => {
    if (busy || running.current) return;
    const key = retry.current?.prizeId === prizeId ? retry.current.key : crypto.randomUUID();
    retry.current = { prizeId, key };
    const timing = gentle ? GENTLE : SPIN;
    const id = (seq.current += 1);
    const from = angleRef.current;
    const end = stopAngle(from, randomSector(), timing.turns);
    angleRef.current = end;
    running.current = { id, stopped: false, answer: null };
    setTurn({ id, from, end, ms: timing.ms, ease: timing.ease, blur: timing.blur, at: performance.now() });
    setBusy(true);
    setShownId(null);
    setError(null);
    clearTimeout(timer.current);
    timer.current = setTimeout(() => {
      stopped(id);
    }, timing.ms + GRACE_MS);
    mutation.mutate({ prizeId, key, id });
  };

  return { turn, angleRef, shownId, error, busy, start, stopped };
}
