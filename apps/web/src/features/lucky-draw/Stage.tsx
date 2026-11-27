import { useId, type ReactNode } from "react";
import { ELIGIBLE_LABEL, GENDER_LABEL, prizeBlock, wins } from "./draw";
import { spinError } from "./messages";
import type { LuckyDrawDetail, Prize } from "./types";
import type { Spin } from "./useSpin";
import { Wheel } from "./Wheel";

/** The prize's audience as the app's gender chip ("Cả nam và nữ" reads muted). */
export function EligibleChip({ prize }: { prize: Pick<Prize, "eligible"> }) {
  return (
    <span className="gender-chip" data-gender={prize.eligible === "all" ? "unknown" : prize.eligible}>
      {ELIGIBLE_LABEL[prize.eligible]}
    </span>
  );
}

/**
 * The stage (spec lucky-draw §5): the wheel on the maroon of the app's chrome, the prize being drawn,
 * "Quay", and the result once the wheel has stopped. The same block fills the presentation, with a
 * picker for the prize (`pick`).
 */
export function Stage({
  draw,
  prize,
  spin,
  onForfeit,
  pick,
  children,
}: {
  draw: LuckyDrawDetail;
  /** The prize "Quay" draws for, `null` when none can be drawn. */
  prize: Prize | null;
  spin: Spin;
  onForfeit: (resultId: string) => void;
  /** Presentation: choose the prize on the stage itself. */
  pick?: (prizeId: string) => void;
  /** Under the result (the presentation's summary). */
  children?: ReactNode;
}) {
  const id = useId();
  const empty = draw.participants.length === 0 || draw.prizes.length === 0;
  const hint = empty
    ? "Thêm người tham gia và giải thưởng để bắt đầu quay."
    : prize
      ? null
      : draw.finished
        ? "Đã trao hết các suất có thể trao. Bấm “Xem tổng kết” để xem lại ai trúng gì."
        : "Không còn giải nào quay được: thêm suất hoặc thêm người phù hợp.";
  const label = draw.results.length > 0 ? "Quay tiếp" : "Quay";
  const shown = spin.shownId === null ? undefined : wins(draw).find((w) => w.result.id === spin.shownId);
  const canSpin = prize !== null && !spin.busy;

  return (
    <section className="ld-stage chrome" aria-label="Vòng quay">
      <div className="ld-stage-prize">
        {pick ? (
          <label className="ld-stage-pick">
            <span>Giải đang quay</span>
            <select
              value={prize?.id ?? ""}
              disabled={spin.busy}
              onChange={(e) => {
                pick(e.target.value);
              }}
            >
              {prize === null && <option value="">Không còn giải quay được</option>}
              {draw.prizes.map((p) => {
                const block = prizeBlock(p);
                return (
                  <option key={p.id} value={p.id} disabled={block !== null}>
                    {block ? `${p.name} (${block})` : p.name}
                  </option>
                );
              })}
            </select>
          </label>
        ) : (
          <p className="ld-stage-label">{prize ? "Giải đang quay" : "Chưa có giải để quay"}</p>
        )}
        {prize && (
          <>
            {!pick && <p className="ld-stage-name">{prize.name}</p>}
            <p className="ld-stage-meta">
              <EligibleChip prize={prize} />
              <span className="tabular">
                Còn {prize.remaining}/{prize.quantity} suất
              </span>
            </p>
          </>
        )}
      </div>

      <Wheel phase={spin.phase} sector={spin.sector} angleRef={spin.angleRef} still={spin.still} onSettled={spin.settle} />

      <div className="ld-stage-actions">
        <button
          type="button"
          className="btn btn-primary btn-lg"
          aria-busy={spin.busy || undefined}
          aria-disabled={prize === null || undefined}
          aria-describedby={hint ? `${id}-hint` : undefined}
          onClick={() => {
            if (canSpin) spin.start(prize.id);
          }}
        >
          {label}
        </button>
        {hint && (
          <p id={`${id}-hint`} className="ld-stage-hint">
            {hint}
          </p>
        )}
      </div>

      <div className="ld-result">
        {spin.busy && (
          <p className="ld-result-wait" aria-hidden="true">
            Đang quay…
          </p>
        )}
        {/* Read out once the wheel has stopped (spec §7). */}
        <div aria-live="polite" aria-atomic="true">
          {!spin.busy && spin.error !== null && (
            <p role="alert" className="alert">
              {spinError(spin.error)}
            </p>
          )}
          {!spin.busy && shown && (
            <div className="ld-result-win">
              <p className="ld-result-label">
                {shown.result.forfeited_at === null ? `Người trúng · lượt ${String(shown.result.seq)}` : "Vắng mặt, đã trả giải"}
              </p>
              <p className="ld-result-name">
                {shown.person?.name ?? ""}
                {shown.person && <span className="sr-only">, {GENDER_LABEL[shown.person.gender]}</span>}
              </p>
              <p className="ld-result-prize">{shown.prize?.name ?? ""}</p>
              {shown.result.forfeited_at === null && (
                <button
                  type="button"
                  className="btn btn-secondary"
                  onClick={() => {
                    onForfeit(shown.result.id);
                  }}
                >
                  Vắng mặt, trả giải
                </button>
              )}
            </div>
          )}
        </div>
        {!spin.busy && !shown && spin.error === null && !empty && (
          <p className="ld-result-idle">Người trúng hiện ở đây khi vòng quay dừng.</p>
        )}
        {children}
      </div>
    </section>
  );
}
