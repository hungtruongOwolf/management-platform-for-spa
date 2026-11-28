import { useId, type ReactNode, type RefObject } from "react";
import { ELIGIBLE_LABEL, prizeBlock, wins } from "./draw";
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
 * The stage (spec lucky-draw §5): the wheel on the maroon of the app's chrome, the prize being drawn
 * above it, "Quay" under it and one status line. The winner is announced by the popup only (EM:
 * "pop up thông báo người chiến thắng … đừng có hiện người thắng ngay dưới vòng quay"); a screen
 * reader hears "X trúng Y" from the status line. The same block fills the presentation, with a
 * picker for the prize (`pick`).
 */
export function Stage({
  draw,
  prize,
  spin,
  pick,
  buttonRef,
  children,
}: {
  draw: LuckyDrawDetail;
  /** The prize "Quay" draws for, `null` when none can be drawn. */
  prize: Prize | null;
  spin: Spin;
  /** Presentation: choose the prize on the stage itself. */
  pick?: (prizeId: string) => void;
  /** Where focus goes back when the winner's popup closes. */
  buttonRef?: RefObject<HTMLButtonElement | null>;
  /** Under the button (the presentation's summary). */
  children?: ReactNode;
}) {
  const id = useId();
  const empty = draw.participants.length === 0 || draw.prizes.length === 0;
  const hint = empty
    ? "Thêm người tham gia và giải thưởng để bắt đầu quay."
    : prize
      ? null
      : draw.finished
        ? pick
          ? "Đã trao hết các suất có thể trao."
          : "Đã trao hết các suất có thể trao. Bấm “Xem tổng kết” để xem lại ai trúng gì."
        : "Không còn giải nào quay được: thêm suất hoặc thêm người phù hợp.";
  const label = draw.results.length > 0 ? "Quay tiếp" : "Quay";
  const shown = spin.shownId === null ? undefined : wins(draw).find((w) => w.result.id === spin.shownId);

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

      <Wheel spin={spin.turn} angleRef={spin.angleRef} onStopped={spin.stopped} />

      <div className="ld-stage-actions">
        <button
          ref={buttonRef}
          type="button"
          className="btn btn-primary btn-lg"
          aria-busy={spin.busy || undefined}
          aria-disabled={prize === null || undefined}
          aria-describedby={hint ? `${id}-hint` : undefined}
          onClick={() => {
            if (prize && !spin.busy) spin.start(prize.id);
          }}
        >
          {label}
        </button>
        <div className="ld-stage-status" aria-live="polite" aria-atomic="true">
          {spin.busy ? (
            <p className="ld-stage-hint" aria-hidden="true">
              Đang quay…
            </p>
          ) : spin.error ? (
            <p role="alert" className="alert">
              {spinError(spin.error)}
            </p>
          ) : (
            shown && (
              <p className="sr-only">
                {shown.person?.name ?? ""} trúng {shown.prize?.name ?? ""}
              </p>
            )
          )}
          {hint && !spin.busy && (
            <p id={`${id}-hint`} className="ld-stage-hint">
              {hint}
            </p>
          )}
        </div>
      </div>
      {children}
    </section>
  );
}
