import { useId, type ReactNode, type RefObject } from "react";
import { Select, type SelectOption } from "../../components/Select";
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
 * The stage (spec lucky-draw §5): the wheel on the maroon of the app's chrome, the prize being
 * drawn, "Quay" and one status line. The wheel is the page's main thing (EM 2026-11-28): on a wide
 * screen it takes the stage's full height and the prize + button sit beside it (lucky-draw.css,
 * container queries on the stage); on a phone everything stacks. The winner is announced by the
 * popup only; a screen reader hears "X trúng Y" from the status line. The same block fills the
 * presentation, with the app's own select for the prize (`pick`) and the winners (`children`).
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
  /** Beside or under the controls (the presentation's winners). */
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
  const choices: SelectOption<string>[] = draw.prizes.map((p) => {
    const block = prizeBlock(p);
    return block ? { value: p.id, label: p.name, disabled: true, reason: block } : { value: p.id, label: p.name };
  });

  return (
    <section className="ld-stage chrome" aria-label="Vòng quay">
      <div className="ld-stage-grid">
        <div className="ld-stage-prize">
          {pick ? (
            <Select
              label="Giải đang quay"
              className="ld-stage-pick"
              value={prize?.id ?? null}
              options={choices}
              placeholder="Không còn giải quay được"
              disabled={spin.busy}
              onChange={pick}
            />
          ) : (
            <>
              <p className="ld-stage-label">{prize ? "Giải đang quay" : "Chưa có giải để quay"}</p>
              {prize && <p className="ld-stage-name">{prize.name}</p>}
            </>
          )}
          {prize && (
            <p className="ld-stage-meta">
              <EligibleChip prize={prize} />
              <span className="tabular">
                Còn {prize.remaining}/{prize.quantity} suất
              </span>
            </p>
          )}
        </div>

        <div className="ld-wheel-slot">
          <Wheel spin={spin.turn} angleRef={spin.angleRef} onStopped={spin.stopped} />
        </div>

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
        {children && <div className="ld-stage-extra">{children}</div>}
      </div>
    </section>
  );
}
