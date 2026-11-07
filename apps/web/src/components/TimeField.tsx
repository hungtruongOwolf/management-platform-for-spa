import { useId, useState } from "react";

/** "15:00", "1500", "9:30", "930", "9", "15h30" → "15:00" / "09:30" / "09:00" / "15:30"; else null. */
export function parseHm(text: string): string | null {
  const t = text.trim();
  const m = /^(\d{1,2})(?:[:.h](\d{2}))?$/.exec(t) ?? /^(\d{1,2})(\d{2})$/.exec(t);
  if (!m) return null;
  const h = Number(m[1]);
  const min = Number(m[2] ?? "0");
  if (h > 23 || min > 59) return null;
  return `${String(h).padStart(2, "0")}:${String(min).padStart(2, "0")}`;
}

/** Bare digits as typed get their colon ("1500" → "15:00", "930" → "9:30"). */
function withColon(raw: string): string {
  if (!/^\d{3,4}$/.test(raw)) return raw;
  const twoDigitHour = Number(raw.slice(0, 2)) <= 23;
  const cut = twoDigitHour ? 2 : 1;
  return `${raw.slice(0, cut)}:${raw.slice(cut, cut + 2)}`;
}

/**
 * A time of day always in 24-hour "HH:MM" (design-system § Định dạng). The native
 * `<input type=time>` follows the browser's locale: an English Chrome shows AM/PM and typing
 * "15:00" landed on 15:58 (EM live test). The field is text; the parent labels it (wrap it in a
 * `<label>` or pass `aria-label`). `""` means no time (each caller says what that means).
 */
export function TimeField({
  value,
  onChange,
  invalid: invalidFromForm = false,
  className,
  id,
  "aria-label": ariaLabel,
}: {
  /** "HH:MM" or "". */
  value: string;
  onChange: (hm: string) => void;
  /** The form knows the value is wrong (e.g. a missing time). */
  invalid?: boolean;
  className?: string;
  id?: string;
  "aria-label"?: string;
}) {
  const [draft, setDraft] = useState(value);
  const [invalid, setInvalid] = useState(false);
  // The parent changed the value ("Dùng giờ hiện tại", a reset): show it, unless it is what the
  // draft already says ("9:30" stays as typed while it means 09:30).
  const [shown, setShown] = useState(value);
  if (shown !== value) {
    setShown(value);
    if (parseHm(draft) !== value) setDraft(value);
    setInvalid(false);
  }

  const commit = (text: string) => {
    if (text.trim() === "") {
      setInvalid(false);
      if (value !== "") onChange("");
      return;
    }
    const hm = parseHm(text);
    if (hm === null) {
      setInvalid(true);
      return;
    }
    setInvalid(false);
    setDraft(hm);
    if (hm !== value) onChange(hm);
  };

  const errorId = useId();
  return (
    <>
      <input
        id={id}
        type="text"
        inputMode="numeric"
        autoComplete="off"
        placeholder="hh:mm"
        maxLength={5}
        value={draft}
        aria-label={ariaLabel}
        aria-invalid={invalid || invalidFromForm || undefined}
        aria-describedby={invalid ? errorId : undefined}
        className={className}
        onChange={(e) => {
          const raw = e.target.value;
          const text = raw.length > draft.length ? withColon(raw) : raw;
          setDraft(text);
          // A whole "HH:MM" counts at once (Ctrl+Enter may save before the field blurs).
          const hm = /^\d{1,2}[:.h]\d{2}$/.test(text.trim()) ? parseHm(text) : null;
          if (hm !== null) {
            setInvalid(false);
            if (hm !== value) onChange(hm);
          }
        }}
        onBlur={(e) => {
          commit(e.target.value);
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter" && !e.ctrlKey && !e.metaKey) commit(e.currentTarget.value);
        }}
      />
      {invalid && (
        <span id={errorId} role="alert" className="field-error">
          Giờ dạng 24 giờ, ví dụ 15:00.
        </span>
      )}
    </>
  );
}
