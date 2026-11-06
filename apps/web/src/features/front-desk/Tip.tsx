import { useId, useState, type ReactNode } from "react";

/**
 * A toggletip: a small round button that shows a short note on click, tap, Enter or Space
 * (design-system.md "Quy tắc": secondary information stays one quiet control away). The note is
 * the button's description, so screen readers read it with the button. Esc closes it.
 */
export function Tip({
  label,
  mark = "?",
  align = "right",
  children,
}: {
  /** Accessible name of the button, e.g. "Phím tắt". */
  label: string;
  /** The visible glyph. */
  mark?: string;
  /** Which edge of the button the note lines up with. */
  align?: "left" | "right";
  children: ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const id = useId();
  return (
    <div
      className="tip"
      onKeyDown={(e) => {
        if (e.key === "Escape" && open) {
          // Not the page's Esc (drop the customer): this Esc only closes the note.
          e.stopPropagation();
          setOpen(false);
        }
      }}
    >
      <button
        type="button"
        className="tip-button"
        aria-label={label}
        aria-expanded={open}
        aria-describedby={id}
        onClick={() => {
          setOpen((o) => !o);
        }}
        onBlur={() => {
          setOpen(false);
        }}
      >
        {mark}
      </button>
      <div
        id={id}
        role="tooltip"
        className={`tip-body ${align === "left" ? "is-left" : ""}`}
      >
        {children}
      </div>
    </div>
  );
}
