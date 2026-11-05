/** The spa's time zone: business days end at its midnight (the server decides, this only shows it). */
export const SPA_TIME_ZONE = "Asia/Ho_Chi_Minh";

const DAY_MS = 86_400_000;

/** Milliseconds from `now` (the device clock) to the next midnight in `timeZone`. */
export function msUntilMidnight(
  now: Date,
  timeZone: string = SPA_TIME_ZONE,
): number {
  const parts = new Intl.DateTimeFormat("en-GB", {
    timeZone,
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hourCycle: "h23",
  }).formatToParts(now);
  const n = (type: string) =>
    Number(parts.find((p) => p.type === type)?.value ?? 0);
  const elapsed =
    ((n("hour") * 60 + n("minute")) * 60 + n("second")) * 1000 +
    now.getMilliseconds();
  return Math.max(0, DAY_MS - elapsed);
}

/** 3_723_000 → "01:02:03" (seconds rounded up, so 0 shows only at midnight). */
export function hms(ms: number): string {
  const total = Math.ceil(ms / 1000);
  const pad = (v: number) => String(v).padStart(2, "0");
  return `${pad(Math.floor(total / 3600))}:${pad(Math.floor((total % 3600) / 60))}:${pad(total % 60)}`;
}

/** The device is set to another zone than the spa (a note explains which clock counts). */
export function deviceZoneDiffers(
  timeZone: string = SPA_TIME_ZONE,
  now: Date = new Date(),
): boolean {
  const offset = (tz: string | undefined) =>
    new Intl.DateTimeFormat("en-US", {
      timeZone: tz,
      timeZoneName: "longOffset",
    })
      .formatToParts(now)
      .find((p) => p.type === "timeZoneName")?.value;
  return offset(undefined) !== offset(timeZone);
}
