// Record history as the screens use it (ADR-0022): the component and the ledger's sheet stay in a
// lazy chunk, loaded only for those who may see them (`canTrace` first, outside this boundary).
// The request starts at once, alongside the chunk, not after it.
import { usePrefetchQuery } from "@tanstack/react-query";
import { Suspense, lazy, useEffect } from "react";
import type { LedgerSaleSheetProps } from "./LedgerSaleSheet";
import { recordHistoryQuery } from "./query";
import type { RecordTraceProps } from "./RecordTrace";

/** Same module specifier as FrontDeskPage's prefetch: one chunk. */
export const loadRecordHistory = () => import("./chunk");

const RecordTrace = lazy(() => loadRecordHistory().then((m) => ({ default: m.RecordTrace })));
const LedgerSaleSheet = lazy(() => loadRecordHistory().then((m) => ({ default: m.LedgerSaleSheet })));

/**
 * What the meta line shows until the chunk and the reply are in: same box, same words, so nothing
 * moves when they arrive (CLS). Styled by components.css, not by the chunk.
 */
export function TracePlaceholder() {
  return (
    <div className="record-trace" data-row-inert aria-busy="true">
      <p className="record-trace-meta">Đang tải ai ghi, ai sửa…</p>
    </div>
  );
}

export function LazyRecordTrace(props: RecordTraceProps) {
  usePrefetchQuery(recordHistoryQuery(props.record));
  return (
    <Suspense fallback={<TracePlaceholder />}>
      <RecordTrace {...props} />
    </Suspense>
  );
}

export function LazyLedgerSaleSheet(props: LedgerSaleSheetProps) {
  return (
    <Suspense fallback={null}>
      <LedgerSaleSheet {...props} />
    </Suspense>
  );
}

/** Loads the chunk ahead of a click on a screen that offers the history (`enabled` = `canTrace`). */
export function usePreloadRecordHistory(enabled: boolean): void {
  useEffect(() => {
    if (enabled) void loadRecordHistory();
  }, [enabled]);
}
