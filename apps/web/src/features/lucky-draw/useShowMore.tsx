import { useState, type ReactNode } from "react";

/** Rows drawn at first and per "Hiện thêm" (spec §5): 2.000 people stay a light page. */
export const PAGE = 100;

/**
 * The first `PAGE` rows, and a "Hiện thêm" button for the next ones. A new `reset` (another filter,
 * another search) starts again from the first page.
 */
export function useShowMore<T>(rows: readonly T[], reset: string): { shown: readonly T[]; more: ReactNode } {
  const [state, setState] = useState({ reset, limit: PAGE });
  const limit = state.reset === reset ? state.limit : PAGE;
  const left = rows.length - limit;
  return {
    shown: left > 0 ? rows.slice(0, limit) : rows,
    more:
      left > 0 ? (
        <button
          type="button"
          className="btn btn-secondary justify-self-start"
          onClick={() => {
            setState({ reset, limit: limit + PAGE });
          }}
        >
          Hiện thêm ({left.toLocaleString("vi-VN")} dòng nữa)
        </button>
      ) : null,
  };
}
