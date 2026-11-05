import { useEffect, useId, useRef, useState } from "react";
import { can, type Me } from "../features/auth/session";
import { Icon } from "./Icon";
import { ThemePicker } from "./ThemePicker";

/** Given name's initial: Vietnamese names end with it. */
export function initialOf(name: string): string {
  return name.trim().split(/\s+/).at(-1)?.charAt(0).toUpperCase() ?? "";
}

/**
 * Display name of the signed-in role, read from the permissions /me returns. A label only — it
 * never decides access (the API does). /me has no role field yet (open question to the backend).
 */
export function roleLabel(me: Me | null): string {
  if (can(me, "catalog.manage")) return "Chủ spa";
  if (can(me, "visit.record")) return "Quản lý";
  return "Kỹ thuật viên";
}

/**
 * Top-bar account control (EM 2026-11-05 round 3: the old initial + name chip "chưa ổn"). A compact
 * button — initial, and from 1024px the short name and role — opens a panel with who is signed in
 * (name, role, spa), the colour theme, "Khóa nhanh" (sign out and come back to this screen after
 * signing in again: the counter is left safely in two taps) and "Đăng xuất".
 */
export function UserMenu({
  me,
  onLock,
  onLogout,
}: {
  me: Me | null;
  onLock: () => void;
  onLogout: () => void;
}) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const button = useRef<HTMLButtonElement>(null);
  const panelId = useId();
  const name = me?.user.full_name ?? "";
  const role = roleLabel(me);
  const spa = me?.active_tenant?.name ?? "Chưa chọn spa";

  useEffect(() => {
    if (!open) return;
    const onPointer = (e: PointerEvent) => {
      if (root.current && !root.current.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setOpen(false);
        button.current?.focus();
      }
    };
    document.addEventListener("pointerdown", onPointer);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("pointerdown", onPointer);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  return (
    <div
      ref={root}
      className="user-menu"
      // Tabbing out of the panel closes it, like a native menu.
      onBlur={(e) => {
        if (open && !e.currentTarget.contains(e.relatedTarget as Node | null)) setOpen(false);
      }}
    >
      <button
        ref={button}
        type="button"
        className="user-button"
        aria-expanded={open}
        aria-controls={panelId}
        onClick={() => {
          setOpen((o) => !o);
        }}
      >
        <span aria-hidden="true" className="avatar">
          {initialOf(name)}
        </span>
        {/* Visible from 1024px; below that it stays as the button's name for screen readers. */}
        <span className="user-button-text">
          <span className="sr-only">Tài khoản: </span>
          <span className="truncate font-semibold">{name}</span>
          <span className="sr-only">, </span>
          <span className="truncate text-xs text-text-muted">{role}</span>
        </span>
        <Icon name="chevron-down" className="user-button-chevron size-4" />
      </button>
      {open && (
        <div id={panelId} className="user-panel" role="group" aria-label="Tài khoản">
          <div className="user-panel-head">
            <span aria-hidden="true" className="avatar size-11 text-lg">
              {initialOf(name)}
            </span>
            <span className="min-w-0">
              <span className="block truncate font-semibold">{name}</span>
              <span className="block truncate text-sm text-text-muted">
                {role} · {spa}
              </span>
            </span>
          </div>
          <div className="user-panel-section">
            <p className="text-xs font-semibold text-text-muted">Giao diện</p>
            <ThemePicker className="w-full" />
          </div>
          <div className="user-panel-actions">
            <button
              type="button"
              className="menu-item"
              onClick={() => {
                setOpen(false);
                onLock();
              }}
            >
              <Icon name="lock" className="size-[18px]" />
              <span>
                Khóa nhanh
                <span className="block text-xs font-normal text-text-muted">Đăng nhập lại để quay về màn này</span>
              </span>
            </button>
            <button
              type="button"
              className="menu-item"
              onClick={() => {
                setOpen(false);
                onLogout();
              }}
            >
              <Icon name="logout" className="size-[18px]" />
              <span>Đăng xuất</span>
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
