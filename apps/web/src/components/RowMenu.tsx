import {
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent,
} from "react";
import { Icon } from "../app/Icon";

export interface RowMenuItem {
  label: string;
  onSelect: () => void;
  /** Destructive actions ("Hủy lượt") read in the danger colour. */
  danger?: boolean;
}

/** Room the menu needs below its button before it opens upwards instead. */
const OPEN_UP_BELOW = 160;
/** Without the Popover API (jsdom, old browsers) the panel is a plain fixed element: the bare
 * `popover` attribute would only hide it. */
const POPOVER =
  typeof HTMLElement !== "undefined" &&
  typeof HTMLElement.prototype.showPopover === "function";

/**
 * The "⋯" actions of one table row (EM round 5: "chi tiết và hủy thì để vào trong cái ..."): a
 * menu button with the WAI-ARIA menu keyboard model (Enter/Space/↓ open on the first item, ↑/↓/
 * Home/End move, Esc closes back to the button, Tab leaves). The panel is a manual popover, so it
 * sits in the top layer and is never clipped by the table's scrolling wrapper; where `popover` is
 * missing (jsdom) it is a plain fixed panel.
 */
export function RowMenu({
  label,
  items,
  id,
}: {
  label: string;
  items: readonly RowMenuItem[];
  /** The button's id, for a row that sends focus back to it. */
  id?: string;
}) {
  const [at, setAt] = useState<{
    top: number;
    right: number;
    up: boolean;
  } | null>(null);
  const button = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  const menuId = useId();
  const open = at !== null;

  const show = () => {
    const r = button.current?.getBoundingClientRect();
    if (!r) return;
    const up =
      r.bottom + OPEN_UP_BELOW > window.innerHeight && r.top > OPEN_UP_BELOW;
    setAt({
      top: up ? r.top - 4 : r.bottom + 4,
      right: window.innerWidth - r.right,
      up,
    });
  };
  const close = (refocus: boolean) => {
    setAt(null);
    if (refocus) button.current?.focus();
  };

  useLayoutEffect(() => {
    const el = menu.current;
    if (!open || !el) return;
    if (POPOVER && !el.matches(":popover-open")) el.showPopover();
    el.querySelector<HTMLElement>('[role="menuitem"]')?.focus();
  }, [open]);

  // A fixed panel would drift from its row on scroll: close it instead, like a native menu.
  useEffect(() => {
    if (!open) return;
    const onPointer = (e: PointerEvent) => {
      const t = e.target as Node;
      if (!menu.current?.contains(t) && !button.current?.contains(t))
        setAt(null);
    };
    // Only a scroll that moves the row closes it: a scroll event queued before the menu opened
    // (the page shrinking after a form closed clamps the scroll position) must not (e2e voids).
    const anchor = button.current?.getBoundingClientRect();
    const onScroll = (e: Event) => {
      if (e.target instanceof Node && menu.current?.contains(e.target)) return;
      const now = button.current?.getBoundingClientRect();
      if (!anchor || !now || Math.abs(now.top - anchor.top) > 1 || Math.abs(now.left - anchor.left) > 1)
        setAt(null);
    };
    const onResize = () => {
      setAt(null);
    };
    document.addEventListener("pointerdown", onPointer);
    window.addEventListener("scroll", onScroll, true);
    window.addEventListener("resize", onResize);
    return () => {
      document.removeEventListener("pointerdown", onPointer);
      window.removeEventListener("scroll", onScroll, true);
      window.removeEventListener("resize", onResize);
    };
  }, [open]);

  const onMenuKey = (e: KeyboardEvent<HTMLDivElement>) => {
    const all = [
      ...(menu.current?.querySelectorAll<HTMLElement>('[role="menuitem"]') ??
        []),
    ];
    const i = all.indexOf(document.activeElement as HTMLElement);
    const go = (n: number) => {
      e.preventDefault();
      all[(n + all.length) % all.length]?.focus();
    };
    if (e.key === "ArrowDown") go(i + 1);
    else if (e.key === "ArrowUp") go(i - 1);
    else if (e.key === "Home") go(0);
    else if (e.key === "End") go(all.length - 1);
    else if (e.key === "Escape") {
      e.preventDefault();
      close(true);
    } else if (e.key === "Tab") close(false);
  };

  return (
    <>
      <button
        ref={button}
        id={id}
        type="button"
        className="row-menu-button"
        aria-label={label}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={open ? menuId : undefined}
        onClick={() => {
          if (open) close(false);
          else show();
        }}
        onKeyDown={(e) => {
          if (e.key === "ArrowDown" && !open) {
            e.preventDefault();
            show();
          }
        }}
      >
        <Icon name="dots" className="size-5" />
      </button>
      {at && (
        <div
          ref={menu}
          id={menuId}
          role="menu"
          aria-label={label}
          popover={POPOVER ? "manual" : undefined}
          className="row-menu"
          style={{
            top: at.top,
            right: at.right,
            translate: at.up ? "0 -100%" : undefined,
          }}
          onKeyDown={onMenuKey}
        >
          {items.map((item) => (
            <button
              key={item.label}
              type="button"
              role="menuitem"
              tabIndex={-1}
              className={`menu-item ${item.danger ? "text-danger" : ""}`}
              onClick={() => {
                close(false);
                item.onSelect();
              }}
            >
              {item.label}
            </button>
          ))}
        </div>
      )}
    </>
  );
}
