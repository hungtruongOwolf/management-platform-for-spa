// "Ai ghi, ai sửa" on one record (ADR-0022, design record-history §1.2): the meta line "Ghi bởi Hà
// · 10:07 · đã sửa 1 lần" and, opened, the record's own history (who, when, what, why), oldest
// first. Always the last element of a record's details. Lazy chunk: screens render it through
// `LazyRecordTrace` (lazy.tsx) after checking `canTrace`.
import type { RecordHistory } from "@vpyd/api-client";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useEffectEvent, useId, useRef, useState } from "react";
import { ApiError } from "../../lib/api";
import { recordHistoryQuery, type RecordRef } from "./query";
import { changeLines, lineText, metaText, recordDayOf, whenText } from "./text";

export interface RecordTraceProps {
  record: RecordRef;
  /** The record as named in the list's label: "lượt 10:05 của chị Lê Thị Mai". */
  noun: string;
  /**
   * `line` (one record open): the meta line is a disclosure button over the list, closed unless
   * `defaultOpen`. `external`: the screen's own control (a card's time, a "Lịch sử" button) opened
   * it and points at `id` with `aria-controls`; the meta line and the list show as they are.
   */
  mode?: "line" | "external";
  defaultOpen?: boolean;
  /** The record's own day (a visit's business day): a line on another day says its date too. */
  day?: string | undefined;
  /** `external`: the id the opening control's `aria-controls` names. */
  id?: string | undefined;
  /**
   * The history right was taken away mid-session (`403`): the block hides, and if focus was in it
   * (or on the control that opened it) it moves here, the record's title or row control.
   */
  focusOnHide?: (() => HTMLElement | null | undefined) | undefined;
  /** Also on `403`, after hiding (e.g. close the sheet the history lives in). */
  onForbidden?: (() => void) | undefined;
  /** Focus the disclosure button once shown (opened from a row's ⋯ menu, whose item is gone). */
  autoFocus?: boolean | undefined;
}

export const LOADING_TEXT = "Đang tải ai ghi, ai sửa…";

function Lines({ h, noun, recordDay }: { h: RecordHistory; noun: string; recordDay: string }) {
  if (h.items.length === 0) return <p>Chưa có lịch sử thay đổi.</p>;
  return (
    <>
      {h.omitted > 0 && <p>Còn {h.omitted} thay đổi cũ hơn.</p>}
      <ol className="record-trace-list" aria-label={`Lịch sử ${noun}`}>
        {h.items.map((e) => (
          <li key={e.id}>
            <time dateTime={e.at}>{whenText(e.at, e.day, recordDay)}</time>
            <span className="record-trace-line">{lineText(e)}</span>
            {changeLines(e).map((c, i) => (
              <span key={i} className="record-trace-change">
                {c}
              </span>
            ))}
          </li>
        ))}
      </ol>
    </>
  );
}

function Failure({ error, onRetry }: { error: unknown; onRetry: () => void }) {
  if (error instanceof ApiError && error.status === 404)
    return <p className="record-trace-meta">Không xem được lịch sử của mục này.</p>;
  return (
    <p className="record-trace-meta">
      Không tải được lịch sử.
      <button type="button" className="btn btn-ghost" onClick={onRetry}>
        Thử lại
      </button>
    </p>
  );
}

export function RecordTrace({
  record,
  noun,
  mode = "line",
  defaultOpen = false,
  day,
  id,
  focusOnHide,
  onForbidden,
  autoFocus = false,
}: RecordTraceProps) {
  const queryClient = useQueryClient();
  const history = useQuery(recordHistoryQuery(record));
  const [open, setOpen] = useState(defaultOpen);
  const generated = useId();
  const regionId = id ?? `rt-${generated}`;
  const toggle = useRef<HTMLButtonElement>(null);
  const forbidden = history.error instanceof ApiError && history.error.status === 403;

  const hidden = useEffectEvent(() => {
    // The switch changed under this session (spec edge case 11): the next `/me` drops the right and
    // every screen stops offering the history.
    void queryClient.invalidateQueries({ queryKey: ["me"] });
    const active = document.activeElement;
    // Focus that was on the removed button falls back to <body>.
    const lost = !active || active === document.body || active.getAttribute("aria-controls") === regionId;
    if (lost) focusOnHide?.()?.focus();
    onForbidden?.();
  });
  useEffect(() => {
    if (forbidden) hidden();
  }, [forbidden]);

  useEffect(() => {
    if (autoFocus) toggle.current?.focus();
  }, [autoFocus]);

  if (forbidden) return null;
  const data = history.data;
  const retry = () => void history.refetch();

  if (mode === "external") {
    return (
      <div id={regionId} className="record-trace" data-row-inert aria-busy={history.isPending || undefined}>
        {data ? (
          <>
            <p className="record-trace-meta">{metaText(data, day)}</p>
            <Lines h={data} noun={noun} recordDay={recordDayOf(data, day)} />
          </>
        ) : history.isError ? (
          <Failure error={history.error} onRetry={retry} />
        ) : (
          <p className="record-trace-meta">{LOADING_TEXT}</p>
        )}
      </div>
    );
  }

  return (
    <div className="record-trace" data-row-inert>
      {!data && history.isError ? (
        <Failure error={history.error} onRetry={retry} />
      ) : (
        <>
          <button
            ref={toggle}
            type="button"
            className="record-trace-toggle"
            aria-expanded={open}
            aria-controls={regionId}
            aria-busy={data ? undefined : true}
            onClick={() => {
              setOpen((o) => !o);
            }}
          >
            <span>{data ? metaText(data, day) : LOADING_TEXT}</span>
            <span className="disclosure-arrow" aria-hidden="true" />
          </button>
          <div id={regionId} hidden={!open}>
            {data && <Lines h={data} noun={noun} recordDay={recordDayOf(data, day)} />}
          </div>
        </>
      )}
    </div>
  );
}
