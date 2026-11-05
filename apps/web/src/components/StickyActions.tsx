import type { ReactNode } from "react";

/**
 * The screen's primary action(s). Phones: pinned above the tab bar (and the home indicator), full
 * width, thumb reach — the button stays visible while the form scrolls (M3 bottom app bar, Baymard
 * sticky primary action). Wider screens: an ordinary right-aligned row at the end of the form,
 * unless `always` keeps it pinned there too (long forms).
 *
 * Put the primary button last (right on desktop, right/bottom on phones). At most one primary.
 */
export function StickyActions({
  children,
  summary,
  label = "Thao tác",
  always = false,
  className = "",
}: {
  children: ReactNode;
  /** Short context left of the buttons, e.g. "Tổng 1.200.000đ" (tabular). */
  summary?: ReactNode;
  label?: string;
  always?: boolean;
  className?: string;
}) {
  return (
    <div role="group" aria-label={label} className={`sticky-actions ${always ? "is-always" : ""} ${className}`}>
      {summary && <div className="sticky-actions-summary">{summary}</div>}
      <div className="sticky-actions-buttons">{children}</div>
    </div>
  );
}
