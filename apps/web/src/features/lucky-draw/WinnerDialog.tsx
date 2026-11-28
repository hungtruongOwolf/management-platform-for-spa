import { useEffect, useId, useRef } from "react";
import { useBackdropClose, useModalDialog } from "../../components/useModalDialog";
import { GENDER_LABEL, wins } from "./draw";
import type { LuckyDrawDetail, Prize } from "./types";

/**
 * The winner, announced in the middle of the screen once the wheel has stopped (EM: "mỗi lần 1 người
 * trúng thưởng thì pop up lên cái thông báo chính giữa màn hình"): "Chúc mừng" + the spin number,
 * the name very large with the gender chip, the prize, what is left of it. On the chrome maroon with
 * gold only as an accent; a short fade + scale in (fade only with reduced motion). A modal
 * `<dialog>` centred on every screen, over the presentation too (`big`: TV-size type).
 *
 * "Quay tiếp" closes it and spins again; when the spin finished the program, "Xem tổng kết" takes
 * its place. Esc, the backdrop and "Đóng" close it; the page then gives focus back to "Quay".
 */
export function WinnerDialog({
  draw,
  resultId,
  next,
  big,
  onNext,
  onSummary,
  onForfeit,
  onClose,
}: {
  draw: LuckyDrawDetail;
  resultId: string;
  /** The prize "Quay tiếp" would draw; `null` when nothing is left to spin. */
  next: Prize | null;
  big: boolean;
  onNext: () => void;
  onSummary: () => void;
  onForfeit: (resultId: string) => void;
  onClose: () => void;
}) {
  const ref = useModalDialog(true);
  const backdrop = useBackdropClose(onClose);
  const id = useId();
  const primary = useRef<HTMLButtonElement>(null);
  // After showModal (the effect above): the next step, or "Đóng" when there is none.
  useEffect(() => {
    primary.current?.focus();
  }, []);
  const win = wins(draw).find((w) => w.result.id === resultId);
  if (!win) return null;
  const { person, prize, result } = win;
  const forfeited = result.forfeited_at !== null;

  return (
    <dialog
      ref={ref}
      className={`ld-winner chrome${big ? " is-big" : ""}`}
      aria-labelledby={`${id}-title`}
      aria-describedby={`${id}-who`}
      onClose={onClose}
      {...backdrop}
    >
      <div className="ld-winner-body">
        <p className="ld-winner-kicker">
          <span id={`${id}-title`} className="ld-winner-title">
            Chúc mừng
          </span>
          <span className="ld-pill tabular">Lượt {result.seq}</span>
        </p>
        <span aria-hidden="true" className="ld-winner-rule" />
        <div id={`${id}-who`} className="ld-winner-who">
          <p className="ld-winner-name">{person?.name ?? ""}</p>
          {person && (
            <p>
              <span className="gender-chip" data-gender={person.gender}>
                {GENDER_LABEL[person.gender]}
              </span>
            </p>
          )}
          <p className="ld-winner-won">trúng</p>
          <p className="ld-winner-prize">{prize?.name ?? ""}</p>
          {prize?.description && <p className="ld-winner-desc">{prize.description}</p>}
        </div>
        {prize && (
          <p className="ld-winner-left tabular">
            Còn {prize.remaining}/{prize.quantity} suất
          </p>
        )}
        <div className="ld-winner-actions">
          {draw.finished ? (
            <button ref={primary} type="button" className="btn btn-primary btn-lg btn-block" onClick={onSummary}>
              Xem tổng kết
            </button>
          ) : next ? (
            <button ref={primary} type="button" className="btn btn-primary btn-lg btn-block" onClick={onNext}>
              Quay tiếp
            </button>
          ) : (
            <p className="ld-winner-desc">Không còn giải nào quay được.</p>
          )}
          <div className="ld-winner-more">
            {!forfeited && (
              <button
                type="button"
                className="btn btn-danger-outline"
                onClick={() => {
                  onForfeit(result.id);
                }}
              >
                Vắng mặt, trả giải
              </button>
            )}
            <button ref={draw.finished || next ? undefined : primary} type="button" className="btn btn-ghost" onClick={onClose}>
              Đóng
            </button>
          </div>
        </div>
      </div>
    </dialog>
  );
}
