import { useEffect, useState } from "react";
import { deviceZoneDiffers, hms, msUntilMidnight } from "./countdown";

/**
 * "Còn hh:mm:ss để nhập số liệu hôm nay" (EM round 4 item 5): a live count on the device clock to
 * the spa's midnight (Asia/Ho_Chi_Minh), when the day locks for non-owners. Informational only:
 * the server decides the day. Fixed width (tabular digits) so the ticking never shifts the layout.
 */
export function DayCountdown({ owner }: { owner: boolean }) {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const t = setInterval(() => {
      setNow(new Date());
    }, 1000);
    return () => {
      clearInterval(t);
    };
  }, []);
  const left = msUntilMidnight(now);
  const [zoneDiffers] = useState(() => deviceZoneDiffers());
  return (
    <p className="grid justify-items-end gap-0.5 text-right text-sm">
      <span>
        Còn{" "}
        <strong
          className={`tabular-nums ${left < 3_600_000 && !owner ? "text-danger" : ""}`}
          // Announcing every second would drown a screen reader: the text is read on focus/visit.
          aria-live="off"
        >
          {hms(left)}
        </strong>{" "}
        để nhập số liệu hôm nay
      </span>
      {owner && (
        <span className="text-xs text-text-muted">
          Chủ spa vẫn nhập bù được ngày đã qua.
        </span>
      )}
      {zoneDiffers && (
        <span className="text-xs text-text-muted">
          Giờ máy khác giờ spa (GMT+7): đồng hồ đếm theo giờ spa.
        </span>
      )}
    </p>
  );
}
