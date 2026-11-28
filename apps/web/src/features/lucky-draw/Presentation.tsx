import { useEffect, useEffectEvent, useId, type RefObject } from "react";
import { useModalDialog } from "../../components/useModalDialog";
import { Stage } from "./Stage";
import { SummaryBody } from "./Summary";
import type { LuckyDrawDetail, Prize } from "./types";
import type { Spin } from "./useSpin";

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
        {draw.finished && !spin.busy && (
          <div className="ld-present-summary">
            <h3 className="ld-summary-heading">Tổng kết chương trình</h3>
            <SummaryBody draw={draw} headingLevel={4} />
          </div>
        )}
      </Stage>
    </dialog>
  );
}
