import { useMutation, useQueryClient } from "@tanstack/react-query";
import { ApiError } from "../../lib/api";
import { LIST_KEY, drawKey } from "./api";
import type { LuckyDrawDetail } from "./types";

/**
 * A write inside one program. The server answers with the whole program, which replaces the cache
 * as is (architecture/lucky-draw.md §4); the list's counts are only marked stale. A refusal because
 * the data moved on (409, 412) refetches the program, so the screen shows what is true now.
 */
export function useDrawWrite<V>(
  drawId: string,
  write: (vars: V) => Promise<LuckyDrawDetail>,
  onDone?: (draw: LuckyDrawDetail, vars: V) => void,
) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: write,
    onSuccess: (draw, vars) => {
      queryClient.setQueryData(drawKey(drawId), draw);
      void queryClient.invalidateQueries({ queryKey: LIST_KEY, exact: true });
      onDone?.(draw, vars);
    },
    onError: (error) => {
      if (error instanceof ApiError && (error.status === 409 || error.status === 412))
        void queryClient.invalidateQueries({ queryKey: drawKey(drawId), exact: true });
    },
  });
}
