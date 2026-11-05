// A file, never a data: URI — Chrome refuses <use> on data URIs — and hashed, so it is cached for good.
import sprite from "../assets/icons.svg?no-inline";

export type IconName =
  | "home"
  | "desk"
  | "users"
  | "user-clock"
  | "package"
  | "chart"
  | "menu-book"
  | "layers"
  | "settings"
  | "search"
  | "lock"
  | "chevron-left"
  | "chevron-right"
  | "sidebar"
  | "more"
  | "calendar"
  | "clock"
  | "sun"
  | "moon"
  | "monitor"
  | "plus"
  | "x"
  | "check"
  | "alert"
  | "arrow-up"
  | "arrow-down"
  | "arrow-right"
  | "history"
  | "wallet"
  | "sparkle"
  | "leaf";

/** Decorative line icon from the sprite; the control it sits in carries the accessible name. */
export function Icon({ name, className = "size-5" }: { name: IconName; className?: string }) {
  return (
    <svg aria-hidden="true" focusable="false" className={`icon ${className}`}>
      <use href={`${sprite}#${name}`} />
    </svg>
  );
}
