import type { CustomerSummary } from "@vpyd/api-client";
import { useQuery } from "@tanstack/react-query";
import { useEffect, useId, useState, type ReactNode } from "react";
import { CustomerName } from "../../app/CustomerName";
import { Icon } from "../../app/Icon";
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
  aside,
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
  /** A small control at the right of the label row (front desk: the shortcuts tip). */
  aside?: ReactNode;
}) {
  const [text, setText] = useState(query);
  const [term, setTerm] = useState(query);
  const [active, setActive] = useState(0);
  // The text a customer was picked from: its hits stay hidden until the text changes (the pick
  // shows below; a second Enter must not re-pick and reset what was started).
  const [pickedAt, setPickedAt] = useState<string | null>(null);
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
  const shown = pickedAt === null || pickedAt !== text;
  const hits = shown ? (results.data ?? []) : [];
  const pick = (c: CustomerSummary) => {
    setPickedAt(text);
    onSelect(c);
  };

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
        pick(hit);
      }
    }
  };

  return (
    <div className="grid gap-2">
      <div className="search-head">
        <label htmlFor={`${listId}-input`} className="text-sm font-semibold">
          Tìm khách theo SĐT hoặc tên
        </label>
        {aside}
      </div>
      <div className="search-row">
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
          placeholder="Không cần gõ dấu"
          className="min-h-13 w-0 rounded-[10px] border-2 border-border bg-surface px-4 text-lg focus:border-primary"
        />
        <button
          type="button"
          onClick={() => {
            onCreate(text);
          }}
          className="btn btn-secondary min-h-13"
        >
          <Icon name="plus" className="size-[18px]" />
          Khách mới
        </button>
      </div>
      {searchable && results.isError && (
        <p role="alert" className="text-sm text-danger">
          {messageFor(results.error)}
        </p>
      )}
      {searchable && results.isPending && (
        <div aria-hidden="true" className="grid gap-1">
          <div className="h-11 skeleton" />
          <div className="h-11 skeleton" />
        </div>
      )}
      {searchable && shown && results.isSuccess && (
        <>
          {hits.length > 0 ? (
            <ul
              id={listId}
              role="listbox"
              aria-label="Kết quả tìm khách"
              className="divide-y divide-border overflow-hidden rounded-[10px] border border-border"
            >
              {hits.map((c, i) => (
                <li
                  key={c.id}
                  id={`${listId}-${c.id}`}
                  role="option"
                  aria-selected={i === active}
                  onClick={() => {
                    pick(c);
                  }}
                  className="flex min-h-11 cursor-pointer items-center justify-between gap-2 px-3 py-2 aria-selected:bg-primary-soft hover:bg-surface-2"
                >
                  <CustomerName
                    name={c.full_name}
                    gender={c.gender}
                    className="font-semibold"
                  />
                  <span className="text-sm text-text-muted tabular">
                    {c.phone}
                  </span>
                </li>
              ))}
            </ul>
          ) : (
            <p className="text-text-muted">
              Không tìm thấy khách nào. Bấm “Khách mới” để tạo hồ sơ với chữ đã
              gõ.
            </p>
          )}
        </>
      )}
    </div>
  );
}
