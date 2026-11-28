import { useEffect, useId, useLayoutEffect, useRef, useState, type KeyboardEvent } from "react";
import "./select.css";

export interface SelectOption<V extends string> {
  value: V;
  label: string;
  /** Shown, not choosable; `reason` says why ("Hết suất"). */
  disabled?: boolean;
  reason?: string;
}

/** Without the Popover API (jsdom, old browsers) the list is a plain fixed panel. */
const POPOVER = typeof HTMLElement !== "undefined" && typeof HTMLElement.prototype.showPopover === "function";
/** Room the list needs below its button before it opens upwards instead. */
const OPEN_UP_BELOW = 240;
/** Keys typed within this pause search together ("go" → "Gội đầu"). */
const TYPEAHEAD_MS = 500;

const fold = (s: string) =>
  s
    .normalize("NFD")
    .replace(/\p{Diacritic}/gu, "")
    .replace(/[đĐ]/g, "d")
    .toLowerCase();

/**
 * The app's own select (EM 2026-11-28: the browser's dropdown "khá là khác biệt so với UI của toàn
 * trang"): the WAI-ARIA APG select-only combobox. A button that shows the value, and a listbox in
 * the top layer (Popover API), so it is never clipped, also inside a modal `<dialog>` such as the
 * presentation. Focus stays on the button (`aria-activedescendant`).
 *
 * Keys: ↓ / ↑ / Enter / Space open; then ↑ ↓ Home End PageUp PageDown move, typing jumps to the
 * first match (no accents needed), Enter / Space choose, Esc closes without change, Tab chooses and
 * moves on. Disabled options are listed with their reason and cannot be chosen.
 */
export function Select<V extends string>({
  label,
  value,
  options,
  onChange,
  disabled = false,
  placeholder = "Chọn…",
  className = "",
}: {
  label: string;
  value: V | null;
  options: readonly SelectOption<V>[];
  onChange: (value: V) => void;
  disabled?: boolean;
  placeholder?: string;
  className?: string;
}) {
  const id = useId();
  const button = useRef<HTMLButtonElement>(null);
  const list = useRef<HTMLUListElement>(null);
  const [place, setPlace] = useState<{ left: number; top: number; width: number; up: boolean } | null>(null);
  const [active, setActive] = useState(0);
  const typed = useRef({ text: "", at: 0 });
  const open = place !== null;
  const selected = options.findIndex((o) => o.value === value);
  const current = selected >= 0 ? options[selected] : undefined;
  const optionId = (i: number) => `${id}-o${String(i)}`;

  const show = (at: number) => {
    const r = button.current?.getBoundingClientRect();
    if (!r || disabled) return;
    const up = r.bottom + OPEN_UP_BELOW > window.innerHeight && r.top > OPEN_UP_BELOW;
    setActive(Math.max(0, Math.min(options.length - 1, at)));
    setPlace({ left: r.left, top: up ? r.top - 4 : r.bottom + 4, width: r.width, up });
  };
  const close = () => {
    setPlace(null);
  };
  const choose = (i: number) => {
    const option = options[i];
    if (!option || option.disabled) return false;
    close();
    if (option.value !== value) onChange(option.value);
    return true;
  };

  useLayoutEffect(() => {
    const el = list.current;
    if (!open || !el) return;
    if (POPOVER && !el.matches(":popover-open")) el.showPopover();
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const option = document.getElementById(optionId(active));
    if (option && "scrollIntoView" in option) option.scrollIntoView({ block: "nearest" });
    // `optionId` only reads the stable `id`.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, active]);

  // A fixed list would drift from its button: close it on an outside press, a scroll or a resize.
  useEffect(() => {
    if (!open) return;
    const onPointer = (e: PointerEvent) => {
      const t = e.target as Node;
      if (!list.current?.contains(t) && !button.current?.contains(t)) close();
    };
    const onScroll = (e: Event) => {
      if (!(e.target instanceof Node && list.current?.contains(e.target))) close();
    };
    document.addEventListener("pointerdown", onPointer);
    window.addEventListener("scroll", onScroll, true);
    window.addEventListener("resize", close);
    return () => {
      document.removeEventListener("pointerdown", onPointer);
      window.removeEventListener("scroll", onScroll, true);
      window.removeEventListener("resize", close);
    };
  }, [open]);

  /** The next option, from `from`, whose label starts with what was typed (accents ignored). */
  const typeahead = (key: string, from: number) => {
    const now = Date.now();
    const t = typed.current;
    t.text = now - t.at > TYPEAHEAD_MS ? key : t.text + key;
    t.at = now;
    const q = fold(t.text);
    const start = t.text.length === 1 ? from + 1 : from;
    for (let k = 0; k < options.length; k += 1) {
      const i = (start + k) % options.length;
      if (fold(options[i]?.label ?? "").startsWith(q)) return i;
    }
    return -1;
  };

  const onKeyDown = (e: KeyboardEvent<HTMLButtonElement>) => {
    const last = options.length - 1;
    const printable = e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey;
    const typing = printable && (e.key !== " " || Date.now() - typed.current.at < TYPEAHEAD_MS);
    if (!open) {
      if (typing) {
        e.preventDefault();
        const i = typeahead(e.key, selected);
        show(i >= 0 ? i : Math.max(selected, 0));
      } else if (["ArrowDown", "ArrowUp", "Enter", " "].includes(e.key)) {
        e.preventDefault();
        show(Math.max(selected, 0));
      } else if (e.key === "Home") {
        e.preventDefault();
        show(0);
      } else if (e.key === "End") {
        e.preventDefault();
        show(last);
      }
      return;
    }
    const move = (i: number) => {
      e.preventDefault();
      setActive(Math.max(0, Math.min(last, i)));
    };
    if (typing) {
      e.preventDefault();
      const i = typeahead(e.key, active);
      if (i >= 0) setActive(i);
    } else if (e.key === "ArrowDown" && e.altKey) move(active);
    else if (e.key === "ArrowUp" && e.altKey) {
      e.preventDefault();
      if (!choose(active)) close();
    } else if (e.key === "ArrowDown") move(active + 1);
    else if (e.key === "ArrowUp") move(active - 1);
    else if (e.key === "Home") move(0);
    else if (e.key === "End") move(last);
    else if (e.key === "PageDown") move(active + 10);
    else if (e.key === "PageUp") move(active - 10);
    else if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      choose(active);
    } else if (e.key === "Escape") {
      e.preventDefault();
      close();
    } else if (e.key === "Tab") {
      if (!choose(active)) close();
    }
  };

  return (
    <div className={`select ${className}`}>
      <span id={`${id}-label`} className="select-label">
        {label}
      </span>
      <button
        ref={button}
        type="button"
        role="combobox"
        className="select-button"
        aria-labelledby={`${id}-label`}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={open ? `${id}-list` : undefined}
        aria-activedescendant={open ? optionId(active) : undefined}
        aria-disabled={disabled || undefined}
        onClick={() => {
          if (open) close();
          else show(Math.max(selected, 0));
        }}
        onKeyDown={onKeyDown}
      >
        <span className={current ? "select-value" : "select-value is-placeholder"}>{current?.label ?? placeholder}</span>
      </button>
      {place && (
        <ul
          ref={list}
          id={`${id}-list`}
          role="listbox"
          aria-labelledby={`${id}-label`}
          popover={POPOVER ? "manual" : undefined}
          className="select-list"
          style={{
            left: place.left,
            top: place.top,
            minWidth: place.width,
            translate: place.up ? "0 -100%" : undefined,
          }}
        >
          {options.map((o, i) => (
            <li
              key={o.value}
              id={optionId(i)}
              role="option"
              aria-selected={i === selected}
              aria-disabled={o.disabled || undefined}
              className={`select-option${i === active ? " is-active" : ""}`}
              // Keep focus on the button: the press must not blur it before the click.
              onPointerDown={(e) => {
                e.preventDefault();
              }}
              onPointerMove={() => {
                if (i !== active) setActive(i);
              }}
              onClick={() => {
                choose(i);
              }}
            >
              <span className="select-option-label">{o.label}</span>
              {o.disabled && o.reason && <span className="select-option-reason">{o.reason}</span>}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
