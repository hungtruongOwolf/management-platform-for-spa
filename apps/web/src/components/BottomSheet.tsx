import { useId, useRef, type ReactNode } from "react";
import { Icon } from "../app/Icon";
import { useModalDialog } from "./useModalDialog";

export { useModalDialog };

/** Drag distance (px) on the handle/header that dismisses the sheet. */
const DISMISS_DRAG = 96;

export interface BottomSheetProps {
  open: boolean;
  onClose: () => void;
  /** Visible heading and the dialog's accessible name. */
  title: string;
  /** One line under the title (e.g. the customer the form is for). */
  description?: ReactNode;
  children: ReactNode;
  /** Primary/secondary buttons pinned to the bottom edge, above the keyboard and home indicator. */
  footer?: ReactNode;
  /** Wider centred dialog on desktop (forms with two columns). */
  wide?: boolean;
  /** Footer buttons outside a `<form>` in the body submit it with `form="<form id>"`. */
  className?: string;
}

/**
 * Phones: a modal sheet that slides up from the bottom edge, with a grab handle (drag down to
 * close), a scrollable body and a pinned footer (M3 modal bottom sheet, HIG sheet). From 640 px:
 * the same content as a centred dialog. Use it for forms and filters that would otherwise push the
 * page around; keep the page's own state (URL filters) as the source of truth.
 */
export function BottomSheet({ open, onClose, title, description, children, footer, wide = false, className = "" }: BottomSheetProps) {
  const ref = useModalDialog(open);
  const titleId = useId();
  const drag = useRef<{ y: number; dy: number } | null>(null);

  const onPointerDown = (e: React.PointerEvent) => {
    if (e.pointerType === "mouse") return;
    drag.current = { y: e.clientY, dy: 0 };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  };
  const onPointerMove = (e: React.PointerEvent) => {
    const d = drag.current;
    const sheet = ref.current;
    if (!d || !sheet) return;
    d.dy = Math.max(0, e.clientY - d.y);
    sheet.style.transform = `translateY(${String(d.dy)}px)`;
  };
  const onPointerUp = () => {
    const d = drag.current;
    drag.current = null;
    if (ref.current) ref.current.style.transform = "";
    if (d && d.dy > DISMISS_DRAG) onClose();
  };

  return (
    <dialog
      ref={ref}
      aria-labelledby={titleId}
      className={`sheet ${wide ? "sheet-wide" : ""} ${className}`}
      onClose={onClose}
      onClick={(e) => {
        // A click on the backdrop lands on the dialog element itself.
        if (e.target === e.currentTarget) onClose();
      }}
    >
      {open && (
        <div className="sheet-inner">
          <div
            className="sheet-head"
            onPointerDown={onPointerDown}
            onPointerMove={onPointerMove}
            onPointerUp={onPointerUp}
            onPointerCancel={onPointerUp}
          >
            <span aria-hidden="true" className="sheet-handle" />
            <div className="min-w-0 flex-1">
              <h2 id={titleId} className="sheet-title">
                {title}
              </h2>
              {description && <p className="sheet-desc">{description}</p>}
            </div>
            <button type="button" className="icon-btn sheet-close" aria-label="Đóng" onClick={onClose}>
              <Icon name="x" />
            </button>
          </div>
          <div className="sheet-body">{children}</div>
          {footer && <div className="sheet-footer">{footer}</div>}
        </div>
      )}
    </dialog>
  );
}
