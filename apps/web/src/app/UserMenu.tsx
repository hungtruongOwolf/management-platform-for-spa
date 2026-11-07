import { Suspense, lazy, useEffect, useId, useRef, useState } from "react";
import { can, type Me } from "../features/auth/session";
import { Icon } from "./Icon";

const loadPanel = () => import("./UserPanel");
const UserPanel = lazy(() => loadPanel().then((m) => ({ default: m.UserPanel })));

/**
 * Given name's initial: Vietnamese names end with it. Only letters count, so a note in brackets
 * ("Chủ spa (test)") gives "T", not "(".
 */
export function initialOf(name: string): string {
  const word = name.trim().split(/\s+/).filter((w) => /\p{L}/u.test(w)).at(-1) ?? "";
  return /\p{L}/u.exec(word)?.[0].toUpperCase() ?? "";
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
        if (open && !e.currentTarget.contains(e.relatedTarget)) setOpen(false);
      }}
    >
      <button
        ref={button}
        type="button"
        className="user-button"
        aria-expanded={open}
        aria-controls={panelId}
        // The panel is a chunk of its own: fetch it as soon as the button may be used.
        onPointerEnter={() => {
          void loadPanel();
        }}
        onFocus={() => {
          void loadPanel();
        }}
        onClick={() => {
          setOpen((o) => !o);
        }}
      >
        <span aria-hidden="true" className="avatar">
          {initialOf(name)}
        </span>
        {/* Visible from 1024px; below that it stays as the button's name for screen readers. */}
        <span className="user-button-text">
          <span className="sr-only">Tài khoản:</span>{" "}
          <span className="truncate font-semibold">{name}</span>
          <span className="sr-only">,</span>{" "}
          <span className="truncate text-xs text-text-muted">{role}</span>
        </span>
        <Icon name="chevron-down" className="user-button-chevron size-4" />
      </button>
      {open && (
        <Suspense fallback={null}>
          <UserPanel
            id={panelId}
            name={name}
            initial={initialOf(name)}
            role={role}
            spa={spa}
            onLock={() => {
              setOpen(false);
              onLock();
            }}
            onLogout={() => {
              setOpen(false);
              onLogout();
            }}
          />
        </Suspense>
      )}
    </div>
  );
}
