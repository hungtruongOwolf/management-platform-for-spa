import { useEffect, useEffectEvent, useId, type RefObject } from "react";
import { useModalDialog } from "../../components/useModalDialog";
import { Stage } from "./Stage";
import type { LuckyDrawDetail, Prize } from "./types";
import type { Spin } from "./useSpin";
import { PrizeHeading, WinRow, winsByPrize } from "./WinRow";

/**
 * "Trình chiếu" (US7): the stage over the whole screen for a TV or a projector. A modal dialog
 * (top layer, focus kept inside, Esc closes) and the page fullscreen when the browser allows it;
 * leaving fullscreen (Esc) leaves the presentation too. The page gives focus back to its button.
 */
export function Presentation({
  draw,
  prize,
  spin,
  buttonRef,
  onPick,
  onClose,
}: {
  draw: LuckyDrawDetail;
  prize: Prize | null;
  spin: Spin;
  buttonRef: RefObject<HTMLButtonElement | null>;
  onPick: (prizeId: string) => void;
  onClose: () => void;
}) {
  const ref = useModalDialog(true);
  const titleId = useId();
  const leave = useEffectEvent(onClose);

  useEffect(() => {
    const root = document.documentElement;
    let entered = false;
    const left = () => {
      if (entered && !document.fullscreenElement) leave();
    };
    document.addEventListener("fullscreenchange", left);
    if (typeof root.requestFullscreen === "function")
      root.requestFullscreen().then(
        () => {
          entered = true;
        },
        // Refused (iframe, iOS Safari): the dialog still covers the window.
        () => undefined,
      );
    return () => {
      document.removeEventListener("fullscreenchange", left);
      if (document.fullscreenElement) void document.exitFullscreen().catch(() => undefined);
    };
  }, []);

  return (
    <dialog ref={ref} className="ld-present chrome" aria-labelledby={titleId} onClose={onClose}>
      <div className="ld-present-head">
        <h2 id={titleId} className="ld-present-title">
          {draw.name}
        </h2>
        <button type="button" className="btn btn-secondary" onClick={onClose}>
          Thoát trình chiếu
        </button>
      </div>
      <Stage draw={draw} prize={prize} spin={spin} pick={onPick} buttonRef={buttonRef}>
        <PresentationWins draw={draw} fresh={spin.shownId} />
      </Stage>
    </dialog>
  );
}

/**
 * The winners beside the wheel on the screen the room watches: a fixed panel that scrolls inside
 * (EM 2026-11-28), grouped by prize like the page's list, without the row menus.
 */
function PresentationWins({ draw, fresh }: { draw: LuckyDrawDetail; fresh: string | null }) {
  const id = useId();
  const groups = winsByPrize(draw);
  return (
    <section className="ld-present-wins" aria-labelledby={`${id}-title`}>
      <h3 id={`${id}-title`} className="ld-present-wins-title">
        Danh sách trúng thưởng
      </h3>
      <div className="ld-scroll" role="region" aria-labelledby={`${id}-title`} tabIndex={0}>
        {groups.length === 0 ? (
          <p className="empty-note">Chưa có ai trúng.</p>
        ) : (
          <div className="ld-groups">
            {groups.map(({ prize, wins }) => (
              <section key={prize.id} className="ld-group" aria-label={prize.name}>
                <PrizeHeading prize={prize} level={4} />
                <ul className="ld-wins">
                  {wins.map((w) => (
                    <WinRow key={w.result.id} win={w} fresh={w.result.id === fresh} />
                  ))}
                </ul>
              </section>
            ))}
          </div>
        )}
      </div>
    </section>
  );
}
