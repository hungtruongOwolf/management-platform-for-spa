import { useSyncExternalStore } from "react";
import { Icon, type IconName } from "./Icon";
import { applyTheme, readTheme, subscribeTheme, type Theme } from "./prefs";

const THEMES: [Theme, string, IconName][] = [
  ["system", "Theo máy", "monitor"],
  ["light", "Sáng", "sun"],
  ["dark", "Tối", "moon"],
];

/**
 * Light / dark / follow the device, remembered on this device (prefs.ts). No local copy of the
 * value: two pickers on screen (account menu + settings) never disagree.
 */
export function ThemePicker({ className = "" }: { className?: string }) {
  const theme = useSyncExternalStore(subscribeTheme, readTheme);
  return (
    <div className={`segmented ${className}`} role="group" aria-label="Chế độ màu">
      {THEMES.map(([t, label, icon]) => (
        <button
          key={t}
          type="button"
          aria-pressed={theme === t}
          onClick={() => {
            applyTheme(t);
          }}
        >
          <Icon name={icon} className="size-[18px]" />
          {label}
        </button>
      ))}
    </div>
  );
}
