import { useState } from "react";
import { Icon, type IconName } from "./Icon";
import { applyTheme, readTheme, type Theme } from "./prefs";

const THEMES: [Theme, string, IconName][] = [
  ["system", "Theo máy", "monitor"],
  ["light", "Sáng", "sun"],
  ["dark", "Tối", "moon"],
];

/** Light / dark / follow the device, remembered on this device (prefs.ts). */
export function ThemePicker({ className = "" }: { className?: string }) {
  const [theme, setTheme] = useState<Theme>(readTheme);
  return (
    <div className={`segmented ${className}`} role="group" aria-label="Chế độ màu">
      {THEMES.map(([t, label, icon]) => (
        <button
          key={t}
          type="button"
          aria-pressed={theme === t}
          onClick={() => {
            applyTheme(t);
            setTheme(t);
          }}
        >
          <Icon name={icon} className="size-[18px]" />
          {label}
        </button>
      ))}
    </div>
  );
}
