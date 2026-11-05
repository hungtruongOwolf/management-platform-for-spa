import { useEffect, useId, useRef, useState, type ReactNode } from "react";
import { Icon } from "../../app/Icon";

/** Delay between the last keystroke and the URL/API update (one request per pause, not per key). */
export const SEARCH_DEBOUNCE_MS = 300;

/**
 * Search box of a list page. The URL owns the committed value (so back restores it); the box
 * keeps what is being typed and commits after a pause, on Enter, or when cleared. When the URL
 * changes from outside (back/forward), the box follows it.
 */
export function SearchField({
  label,
  placeholder,
  value,
  onCommit,
  minLength = 2,
  className = "",
}: {
  label: string;
  placeholder?: string;
  value: string;
  onCommit: (next: string) => void;
  /** Shorter input is not sent (the API answers 422 `too_short` below 2 characters). */
  minLength?: number;
  className?: string;
}) {
  const id = useId();
  const [draft, setDraft] = useState(value);
  const committed = useRef(value);
  const commitRef = useRef(onCommit);
  useEffect(() => {
    commitRef.current = onCommit;
  }, [onCommit]);

  useEffect(() => {
    if (value !== committed.current) {
      committed.current = value;
      setDraft(value);
    }
  }, [value]);

  const commit = (next: string) => {
    const t = next.trim();
    const sendable = t === "" || t.length >= minLength ? t : committed.current;
    if (sendable === committed.current) return;
    committed.current = sendable;
    commitRef.current(sendable);
  };

  useEffect(() => {
    const timer = setTimeout(() => {
      commit(draft);
    }, SEARCH_DEBOUNCE_MS);
    return () => {
      clearTimeout(timer);
    };
    // `commit` reads refs only; re-arming on every draft change is the debounce.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [draft]);

  const tooShort = draft.trim() !== "" && draft.trim().length < minLength;
  return (
    <div className={`search-field ${className}`}>
      <label htmlFor={id} className="sr-only">
        {label}
      </label>
      <Icon name="search" className="search-field-icon" />
      <input
        id={id}
        type="search"
        value={draft}
        placeholder={placeholder ?? label}
        autoComplete="off"
        aria-describedby={tooShort ? `${id}-hint` : undefined}
        onChange={(e) => {
          setDraft(e.target.value);
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            commit(draft);
          }
        }}
      />
      {draft !== "" && (
        <button
          type="button"
          className="search-field-clear"
          aria-label={`Xóa ô ${label.toLowerCase()}`}
          onClick={() => {
            setDraft("");
            commit("");
          }}
        >
          <Icon name="x" className="size-4" />
        </button>
      )}
      {tooShort && (
        <span id={`${id}-hint`} className="sr-only">
          Gõ ít nhất {minLength} ký tự.
        </span>
      )}
    </div>
  );
}

/** A labelled select in a filter row (native: keyboard, mobile pickers and screen readers for free). */
export function FilterSelect<T extends string>({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: T;
  options: readonly (readonly [T, string])[];
  onChange: (v: T) => void;
}) {
  const id = useId();
  return (
    <div className="filter-select">
      <label htmlFor={id}>{label}</label>
      <select
        id={id}
        value={value}
        onChange={(e) => {
          onChange(e.target.value as T);
        }}
      >
        {options.map(([v, text]) => (
          <option key={v} value={v}>
            {text}
          </option>
        ))}
      </select>
    </div>
  );
}

/** One filter row above the content it scopes (dataviz "filters in one row"); wraps on phones. */
export function FilterBar({ children, label }: { children: ReactNode; label: string }) {
  return (
    <div role="search" aria-label={label} className="filter-bar">
      {children}
    </div>
  );
}
