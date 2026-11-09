import { useEffect, useId, useRef, useState, type ReactNode } from "react";
import { Icon } from "../../app/Icon";
import { BottomSheet } from "../../components/BottomSheet";
import { usePhone } from "../../components/useMediaQuery";

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
  sendable,
  className = "",
}: {
  label: string;
  placeholder?: string;
  value: string;
  onCommit: (next: string) => void;
  /** Shorter input is not sent (the API answers 422 `too_short` below 2 characters). */
  minLength?: number;
  /** A stricter rule than `minLength` (e.g. phone numbers count their national digits, like the API). */
  sendable?: (term: string) => boolean;
  className?: string;
}) {
  const accepts = sendable ?? ((t: string) => t.length >= minLength);
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
    const term = t === "" || accepts(t) ? t : committed.current;
    if (term === committed.current) return;
    committed.current = term;
    commitRef.current(term);
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

  const tooShort = draft.trim() !== "" && !accepts(draft.trim());
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
          {sendable ? "Gõ ít nhất 2 chữ, hoặc 3 số của số điện thoại." : `Gõ ít nhất ${String(minLength)} ký tự.`}
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

/**
 * Search + filters. From 640 px: one inline row (`FilterBar`). Phones: the search box stays inline
 * (it is the first thing people use) and the other filters move into a bottom sheet behind a
 * "Bộ lọc" button that shows how many are set (Baymard mobile filtering, M3 modal bottom sheet).
 * Filters still apply as they change (URL state); the sheet's "Xem kết quả" just closes it.
 */
export function ResponsiveFilters({
  label,
  search,
  children,
  activeCount,
  onReset,
}: {
  label: string;
  /** The `SearchField`, if the page has one. */
  search?: ReactNode;
  /** The other filters (`FilterSelect`, date fields, segmented controls). */
  children: ReactNode;
  /** Filters set away from their default (badge on the button). */
  activeCount: number;
  /** "Xóa bộ lọc" in the sheet; omitted when nothing can be reset. */
  onReset?: () => void;
}) {
  const phone = usePhone();
  const [open, setOpen] = useState(false);
  if (!phone) {
    return (
      <FilterBar label={label}>
        {search}
        {children}
      </FilterBar>
    );
  }
  return (
    <div role="search" aria-label={label} className="filter-row-phone">
      {search}
      <button
        type="button"
        className="filter-trigger"
        aria-haspopup="dialog"
        aria-label={activeCount > 0 ? `Bộ lọc, ${String(activeCount)} đang chọn` : "Bộ lọc"}
        onClick={() => {
          setOpen(true);
        }}
      >
        <Icon name="filter" className="size-[18px]" />
        Bộ lọc
        {activeCount > 0 && <span className="filter-count">{activeCount}</span>}
      </button>
      <BottomSheet
        open={open}
        onClose={() => {
          setOpen(false);
        }}
        title={label}
        footer={
          <>
            {onReset && activeCount > 0 && (
              <button type="button" className="btn btn-secondary" onClick={onReset}>
                Xóa bộ lọc
              </button>
            )}
            <button
              type="button"
              className="btn btn-primary"
              onClick={() => {
                setOpen(false);
              }}
            >
              Xem kết quả
            </button>
          </>
        }
      >
        {children}
      </BottomSheet>
    </div>
  );
}
