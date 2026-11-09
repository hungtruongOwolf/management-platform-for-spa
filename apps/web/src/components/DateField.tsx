import { useId, useRef, useState } from "react";
import { Icon } from "../app/Icon";

/** "2026-11-05" → "05/11/2026"; "" stays "". */
export function isoToDmy(iso: string): string {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(iso);
  return m ? `${m[3] ?? ""}/${m[2] ?? ""}/${m[1] ?? ""}` : "";
}

/** "5/11/2026", "05112026", "05/11/2026" → "2026-11-05"; anything else (or an impossible day) → null. */
export function dmyToIso(text: string): string | null {
  const t = text.trim();
  const m = /^(\d{1,2})[/.-](\d{1,2})[/.-](\d{4})$/.exec(t) ?? /^(\d{2})(\d{2})(\d{4})$/.exec(t);
  if (!m) return null;
  const [d, mo, y] = [Number(m[1]), Number(m[2]), Number(m[3])];
  const date = new Date(Date.UTC(y, mo - 1, d));
  if (date.getUTCFullYear() !== y || date.getUTCMonth() !== mo - 1 || date.getUTCDate() !== d) return null;
  return `${String(y)}-${String(mo).padStart(2, "0")}-${String(d).padStart(2, "0")}`;
}

/**
 * A date always written dd/mm/yyyy (design-system § Định dạng). The native `<input type=date>`
 * shows the browser's locale (mm/dd/yyyy on an English Chrome, EM round 4), so the visible field
 * is text; the calendar button opens the native picker of a hidden date input.
 */
export function DateField({
  label,
  value,
  onChange,
  min,
  max,
  describedBy,
  invalid: invalidFromForm = false,
  className = "filter-field",
  hideLabel = false,
  onTextValidity,
}: {
  label: string;
  /** ISO day `yyyy-mm-dd`, or "" for none. */
  value: string;
  onChange: (iso: string) => void;
  min?: string | undefined;
  max?: string | undefined;
  /** Id(s) of a hint or error under the field (read with it). */
  describedBy?: string | undefined;
  /** The form knows the value is wrong (e.g. a server error). */
  invalid?: boolean;
  className?: string;
  /** The label is for screen readers only: the field sits beside a heading that says what it is. */
  hideLabel?: boolean;
  /** Told whether the typed text is a usable date (false = shown as wrong, `value` unchanged), so
   * a form can refuse to save while the field shows its error. */
  onTextValidity?: (valid: boolean) => void;
}) {
  const id = useId();
  const picker = useRef<HTMLInputElement>(null);
  const [draft, setDraft] = useState(isoToDmy(value));
  const [invalid, setInvalid] = useState(false);
  // The URL (or the parent) changed the value: show it (React "adjust state on prop change").
  const [shown, setShown] = useState(value);
  if (shown !== value) {
    setShown(value);
    setDraft(isoToDmy(value));
    setInvalid(false);
  }

  const commit = (text: string) => {
    if (text.trim() === "") {
      setInvalid(false);
      onTextValidity?.(true);
      if (value !== "") onChange("");
      return;
    }
    const iso = dmyToIso(text);
    const outOfRange = iso !== null && ((min !== undefined && iso < min) || (max !== undefined && iso > max));
    if (iso === null || outOfRange) {
      setInvalid(true);
      onTextValidity?.(false);
      return;
    }
    setInvalid(false);
    onTextValidity?.(true);

    setDraft(isoToDmy(iso));
    if (iso !== value) onChange(iso);
  };

  return (
    <div className={className}>
      <label htmlFor={id} className={hideLabel ? "sr-only" : undefined}>
        {label}
      </label>
      <div className="date-field">
        <input
          id={id}
          type="text"
          inputMode="numeric"
          autoComplete="off"
          placeholder="dd/mm/yyyy"
          maxLength={10}
          value={draft}
          aria-invalid={invalid || invalidFromForm || undefined}
          aria-describedby={[invalid ? `${id}-err` : "", describedBy ?? ""].filter(Boolean).join(" ") || undefined}
          onChange={(e) => {
            // Typing digits gets the slashes inserted ("05112026" → "05/11/2026"); deleting, or
            // typing one's own separators ("5/1/2026"), is left as typed.
            const raw = e.target.value;
            const digits = raw.replace(/\D/g, "");
            const growing = raw.length > draft.length;
            const ownSeparators = /[/.-]/.test(raw.slice(draft.length));
            setDraft(
              growing && !ownSeparators && /^[\d/]*$/.test(raw)
                ? [digits.slice(0, 2), digits.slice(2, 4), digits.slice(4, 8)].filter(Boolean).join("/")
                : raw,
            );
          }}
          onBlur={(e) => {
            commit(e.target.value);
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              commit(e.currentTarget.value);
            }
          }}
        />
        <button
          type="button"
          className="date-field-btn"
          aria-label={`Chọn ${label.toLowerCase()} trên lịch`}
          onClick={() => {
            const p = picker.current;
            if (!p) return;
            if (typeof p.showPicker === "function") {
              try {
                p.showPicker();
                return;
              } catch {
                // Not allowed (e.g. not a user gesture in tests): fall through to focus.
              }
            }
            p.focus();
          }}
        >
          <Icon name="calendar" className="size-[18px]" />
        </button>
        <input
          ref={picker}
          type="date"
          tabIndex={-1}
          aria-hidden="true"
          className="date-field-picker"
          value={value}
          min={min}
          max={max}
          onChange={(e) => {
            onChange(e.target.value);
          }}
        />
      </div>
      {invalid && (
        <span id={`${id}-err`} role="alert" className="text-xs text-danger">
          Ngày dạng dd/mm/yyyy{min || max ? ", trong khoảng cho phép" : ""}.
        </span>
      )}
    </div>
  );
}
