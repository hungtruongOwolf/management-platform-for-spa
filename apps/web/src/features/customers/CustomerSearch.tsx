import type { CustomerSummary } from "@vpyd/api-client";
import { useQuery } from "@tanstack/react-query";
import { useEffect, useId, useState } from "react";
import { CustomerName } from "../../app/CustomerName";
import { messageFor } from "../../lib/errors";
import { isSearchable, searchQuery } from "./api";

/** Debounced phone/name search with keyboard selection. Reused by the front desk (S7). */
export function CustomerSearch({
  onSelect,
  onCreate,
  inputRef,
  autoFocus = false,
  query = "",
  onQueryChange,
}: {
  onSelect: (c: CustomerSummary) => void;
  onCreate: (typed: string) => void;
  /** Lets a screen move focus back to the search box (front desk: after saving, on "/"). */
  inputRef?: React.Ref<HTMLInputElement>;
  /** Focus the search box on mount (continuous data entry, front desk). */
  autoFocus?: boolean;
  /** Text to start from (the URL's `q` on /khach-hang, so Back shows the same hits). */
  query?: string;
  /** Called with the settled (debounced) text, e.g. to keep it in the URL. */
  onQueryChange?: (q: string) => void;
}) {
  const [text, setText] = useState(query);
  const [term, setTerm] = useState(query);
  const [active, setActive] = useState(0);
  const listId = useId();

  useEffect(() => {
    const t = setTimeout(() => {
      setTerm(text);
      setActive(0);
      onQueryChange?.(text);
    }, 250);
    return () => {
      clearTimeout(t);
    };
    // onQueryChange is a callback prop: a new identity each render must not restart the debounce.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [text]);

  const searchable = isSearchable(term);
  const results = useQuery({ ...searchQuery(term), enabled: searchable });
  const hits = results.data ?? [];

  const onKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "ArrowDown" && hits.length) {
      e.preventDefault();
      setActive((i) => (i + 1) % hits.length);
    } else if (e.key === "ArrowUp" && hits.length) {
      e.preventDefault();
      setActive((i) => (i - 1 + hits.length) % hits.length);
    } else if (e.key === "Enter") {
      const hit = hits[active];
      if (hit) {
        e.preventDefault();
        onSelect(hit);
      }
    }
  };

  return (
    <div>
      <label
        htmlFor={`${listId}-input`}
        className="block text-sm font-semibold"
      >
        Tìm khách theo SĐT hoặc tên
      </label>
      <input
        id={`${listId}-input`}
        ref={inputRef}
        type="search"
        autoComplete="off"
        // Mirrors the server's bound on `q` (crm::service::MAX_QUERY_CHARS).
        maxLength={100}
        autoFocus={autoFocus}
        value={text}
        onChange={(e) => {
          setText(e.target.value);
        }}
        onKeyDown={onKeyDown}
        role="combobox"
        aria-expanded={searchable && hits.length > 0}
        aria-controls={listId}
        aria-activedescendant={
          hits[active] ? `${listId}-${hits[active].id}` : undefined
        }
        placeholder="Tên hoặc SĐT, không cần dấu"
        className="min-h-13 w-full rounded-[10px] border-2 border-border bg-surface px-4 text-lg focus:border-primary"
      />
      {searchable && results.isError && (
        <p role="alert" className="mt-2 text-sm text-danger">
          {messageFor(results.error)}
        </p>
      )}
      {searchable && results.isSuccess && (
        <>
          {hits.length > 0 ? (
            <ul
              id={listId}
              role="listbox"
              aria-label="Kết quả tìm khách"
              className="mt-2 divide-y divide-border overflow-hidden rounded-[10px] border border-border"
            >
              {hits.map((c, i) => (
                <li
                  key={c.id}
                  id={`${listId}-${c.id}`}
                  role="option"
                  aria-selected={i === active}
                  onClick={() => {
                    onSelect(c);
                  }}
                  className="flex cursor-pointer justify-between gap-2 px-3 py-2 aria-selected:bg-primary-soft hover:bg-primary-soft"
                >
                  <CustomerName name={c.full_name} gender={c.gender} className="font-semibold" />
                  <span className="text-sm text-text-muted tabular">{c.phone}</span>
                </li>
              ))}
            </ul>
          ) : (
            <p className="mt-2 text-text-muted">Không tìm thấy khách nào.</p>
          )}
        </>
      )}
      <button
        type="button"
        onClick={() => {
          onCreate(text);
        }}
        className="mt-2 min-h-11 rounded-[10px] border border-border px-3 text-sm font-medium hover:bg-surface-2"
      >
        + Khách mới
      </button>
    </div>
  );
}
