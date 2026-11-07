import { Icon } from "./Icon";
import { ThemePicker } from "./ThemePicker";

/**
 * The open account panel of `UserMenu`: its own small chunk, loaded when the button is first
 * hovered, focused or pressed (the startup budget is full, frontend.md "Perf budget").
 */
export function UserPanel({
  id,
  name,
  initial,
  role,
  spa,
  onLock,
  onLogout,
}: {
  id: string;
  name: string;
  initial: string;
  role: string;
  spa: string;
  onLock: () => void;
  onLogout: () => void;
}) {
  return (
    <div id={id} className="user-panel" role="group" aria-label="Tài khoản">
      <div className="user-panel-head">
        <span aria-hidden="true" className="avatar size-11 text-lg">
          {initial}
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
        <button type="button" className="menu-item" onClick={onLock}>
          <Icon name="lock" className="size-[18px]" />
          <span>
            Khóa nhanh
            <span className="block text-xs font-normal text-text-muted">Đăng nhập lại để quay về màn này</span>
          </span>
        </button>
        <button type="button" className="menu-item" onClick={onLogout}>
          <Icon name="logout" className="size-[18px]" />
          <span>Đăng xuất</span>
        </button>
      </div>
    </div>
  );
}
