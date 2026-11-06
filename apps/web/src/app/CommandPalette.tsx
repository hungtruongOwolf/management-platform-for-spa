import { useQuery, useSuspenseQuery } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { useEffect, useId, useState } from "react";
import { can, meQuery } from "../features/auth/session";
import { isSearchable, searchQuery } from "../features/customers/api";
import { Modal } from "./AppShell";
import { Icon, type IconName } from "./Icon";
import { navKey, type NavGroup } from "./nav";

interface Option {
  id: string;
  label: string;
  hint: string;
  icon: IconName;
  go: () => void;
}

/** Vietnamese text without diacritics, lower case: "Hồ sơ" matches "ho so". */
export function fold(s: string): string {
  return s
    .normalize("NFD")
    .replace(/\p{Diacritic}/gu, "")
    .replace(/đ/g, "d")
    .replace(/Đ/g, "D")
    .toLowerCase();
}

/**
 * Ctrl/⌘+K: jump to any screen the user may open, or find a customer by phone or name and open
 * the profile. Lazy chunk; the shell only mounts it while it is open.
 */
export default function CommandPalette({
  groups,
  onClose,
}: {
  groups: NavGroup[];
  onClose: () => void;
}) {
  const { data: me } = useSuspenseQuery(meQuery);
  const navigate = useNavigate();
  const [text, setText] = useState("");
  const [term, setTerm] = useState("");
  const [active, setActive] = useState(0);
  const id = useId();

  useEffect(() => {
    const t = setTimeout(() => {
      setTerm(text);
    }, 200);
    return () => {
      clearTimeout(t);
    };
  }, [text]);

  const searchable = can(me, "customer.read") && isSearchable(term);
  const customers = useQuery({ ...searchQuery(term), enabled: searchable });

  const close = (then?: () => void) => {
    onClose();
    then?.();
  };
  const q = fold(text.trim());
  const screens: Option[] = groups
    .flatMap((g) => g.items.map((i) => ({ ...i, group: g.label })))
    .filter((i) => q === "" || fold(`${i.label} ${i.group}`).includes(q))
    .map((i) => ({
      id: `screen-${navKey(i)}`,
      label: i.label,
      hint: i.group,
      icon: i.icon,
      go: () =>
        void navigate({
          to: i.to,
          ...(i.search ? { search: i.search as never } : {}),
        }),
    }));
  const people: Option[] = searchable
    ? (customers.data ?? []).slice(0, 6).map((c) => ({
        id: `customer-${c.id}`,
        label: c.full_name,
        hint: c.phone,
        icon: "users",
        go: () =>
          void navigate({
            to: "/khach-hang/$customerId",
            params: { customerId: c.id },
          }),
      }))
    : [];
  const options = [...people, ...screens];
  const current = Math.min(active, Math.max(options.length - 1, 0));

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const step = e.key === "ArrowDown" ? 1 : -1;
      setActive(
        (current + step + options.length) % Math.max(options.length, 1),
      );
    } else if (e.key === "Enter") {
      const o = options[current];
      if (o) {
        e.preventDefault();
        close(o.go);
      }
    }
  };

  const section = (title: string, list: Option[], offset: number) =>
    list.length > 0 && (
      <li role="presentation">
        <p className="px-3 pt-3 pb-1 eyebrow" aria-hidden="true">
          {title}
        </p>
        <ul role="group" aria-label={title}>
          {list.map((o, i) => (
            <li
              key={o.id}
              id={`${id}-${o.id}`}
              role="option"
              aria-selected={offset + i === current}
              className="palette-option"
              onMouseEnter={() => {
                setActive(offset + i);
              }}
              onClick={() => {
                close(o.go);
              }}
            >
              <Icon name={o.icon} className="size-5 text-text-muted" />
              <span className="font-medium">{o.label}</span>
              <span className="ml-auto text-sm text-text-muted">{o.hint}</span>
            </li>
          ))}
        </ul>
      </li>
    );

  return (
    <Modal open onClose={onClose} className="palette" label="Tìm nhanh">
      <div className="relative">
        <Icon
          name="search"
          className="pointer-events-none absolute top-1/2 left-4 size-5 -translate-y-1/2 text-text-muted"
        />
        <input
          autoFocus
          type="search"
          role="combobox"
          aria-label="Tìm khách hoặc màn hình"
          aria-expanded={options.length > 0}
          aria-controls={`${id}-list`}
          aria-activedescendant={
            options[current] ? `${id}-${options[current].id}` : undefined
          }
          autoComplete="off"
          maxLength={100}
          placeholder="Tên khách, SĐT hoặc tên màn hình…"
          value={text}
          onChange={(e) => {
            setText(e.target.value);
            setActive(0);
          }}
          onKeyDown={onKeyDown}
          className="min-h-14 w-full border-0 border-b border-border bg-transparent pr-4 pl-12 text-lg shadow-none outline-none focus:shadow-none"
        />
      </div>
      <ul
        id={`${id}-list`}
        role="listbox"
        aria-label="Kết quả"
        className="max-h-[60vh] overflow-y-auto p-2"
      >
        {section("Khách hàng", people, 0)}
        {section("Màn hình", screens, people.length)}
      </ul>
      {searchable && customers.isFetching && people.length === 0 && (
        <div role="status" aria-label="Đang tìm khách" className="grid gap-2 px-3 pb-3">
          <div className="skeleton h-12" />
          <div className="skeleton h-12" />
        </div>
      )}
      {options.length === 0 && !customers.isFetching && (
        <p className="px-5 pb-5 text-text-muted">
          Không có kết quả cho “{text}”. Thử tên khác, 4 số cuối SĐT, hoặc tên màn hình như “Báo cáo”.
        </p>
      )}
      <p className="flex gap-4 border-t border-border bg-surface-2 px-4 py-2 text-xs text-text-muted">
        <span>
          <kbd>↑</kbd> <kbd>↓</kbd> chọn
        </span>
        <span>
          <kbd>Enter</kbd> mở
        </span>
        <span>
          <kbd>Esc</kbd> đóng
        </span>
      </p>
    </Modal>
  );
}
