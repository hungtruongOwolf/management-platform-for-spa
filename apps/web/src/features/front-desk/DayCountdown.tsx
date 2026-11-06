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
          {hms(left)}
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
