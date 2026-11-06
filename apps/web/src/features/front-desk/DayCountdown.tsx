import { useEffect, useState } from "react";
import { Icon } from "../../app/Icon";
import { deviceZoneDiffers, hms, msUntilMidnight } from "./countdown";
import { Tip } from "./Tip";

/**
 * "Còn hh:mm:ss để nhập số liệu hôm nay" (EM round 4 item 5): a live count on the device clock to
 * the spa's midnight (Asia/Ho_Chi_Minh), when the day locks for non-owners. Informational only:
 * the server decides the day. One quiet line; the owner and time-zone notes sit behind ⓘ.
 * Fixed width (tabular digits) so the ticking never shifts the layout.
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
  const notes = [
    "Sau 0 giờ (giờ spa), quản lý không ghi hay sửa được số liệu của ngày đã qua.",
    ...(owner ? ["Chủ spa vẫn nhập bù được ngày đã qua."] : []),
    ...(zoneDiffers
      ? ["Giờ máy khác giờ spa (GMT+7): đồng hồ đếm theo giờ spa."]
      : []),
  ];
  return (
    <div className="day-clock">
      <Icon name="clock" className="size-4 shrink-0" />
      <span>
        Còn{" "}
        <strong
          className={`day-clock-time ${left < 3_600_000 && !owner ? "is-late" : ""}`}
          // Announcing every second would drown a screen reader: the text is read on focus/visit.
          aria-live="off"
        >
          <span className="sr-only">{hms(left)}</span>
          {/* Be Vietnam Pro has no true tabular figures: each digit sits in its own 1ch slot, so the
              line stays still as the seconds tick and the time hugs "Còn" (EM 2026-11-07: an
              8.5ch box left a gap before the time). */}
          <span aria-hidden="true">
            {hms(left)
              .split(":")
              .map((pair, unit) => (
                // Hours, minutes, seconds: a fixed position each, never reordered.
                <span key={unit}>
                  {unit > 0 && ":"}
                  <span className="day-clock-digit">{pair.charAt(0)}</span>
                  <span className="day-clock-digit">{pair.charAt(1)}</span>
                </span>
              ))}
          </span>
        </strong>{" "}
        để nhập số liệu hôm nay
      </span>
      <Tip label="Về giờ khóa số liệu" mark="i">
        {notes.map((n) => (
          <span key={n} className="block">
            {n}
          </span>
        ))}
      </Tip>
    </div>
  );
}
