import { useRef, type KeyboardEvent } from "react";
import { Icon } from "../../app/Icon";
import { STATUSES, STATUS_CODE, STATUS_HINT, STATUS_LABEL, STATUS_WORD, type AttendanceStatus } from "./labels";

/**
 * One person's attendance for a day as four worded options (Đi làm / Nghỉ có phép / Nghỉ đột
 * xuất / Nghỉ không phép), one tap saves. A radiogroup with a roving tab stop: Tab enters on the
 * chosen option, ←/→/↑/↓/Home/End move focus, Space/Enter choose. Arrows only move focus — every
 * choice is a save (PUT /attendance), so walking past three options must not record three.
 *
 * States: chosen = filled with the status colour + check icon (colour is never the only signal);
 * `pending` = the option being saved, shown chosen and pulsing until the server answers (on error the
 * parent's server data still holds the old value, so the choice rolls back); `busy` = another save
 * is running: taps are ignored but focus stays (aria-disabled, not disabled); `disabled` = month
 * closed or future day.
 */
export function AttendanceChoice({
  name,
  value,
  pending,
  busy,
  disabled,
  onChange,
}: {
  /** Whose attendance, for the group's accessible name. */
  name: string;
  value: AttendanceStatus | null;
  /** The status being saved right now for this person, if any. */
  pending: AttendanceStatus | null;
  busy: boolean;
  disabled: boolean;
  onChange: (status: AttendanceStatus) => void;
}) {
  const refs = useRef<(HTMLButtonElement | null)[]>([]);
  const shown = pending ?? value;
  const stop = Math.max(0, shown ? STATUSES.indexOf(shown) : 0);
  const blocked = busy || pending !== null;

  const onKeyDown = (e: KeyboardEvent<HTMLButtonElement>, i: number) => {
    const last = STATUSES.length - 1;
    const next =
      e.key === "ArrowRight" || e.key === "ArrowDown"
        ? (i + 1) % STATUSES.length
        : e.key === "ArrowLeft" || e.key === "ArrowUp"
          ? (i + last) % STATUSES.length
          : e.key === "Home"
            ? 0
            : e.key === "End"
              ? last
              : null;
    if (next === null) return;
    e.preventDefault();
    refs.current[next]?.focus();
  };

  return (
    <>
      <div className="att-choice" role="radiogroup" aria-label={`Chấm công ${name}`} aria-busy={pending !== null || undefined}>
        {STATUSES.map((st, i) => {
          const checked = shown === st;
          return (
            <button
              key={st}
              ref={(el) => {
                refs.current[i] = el;
              }}
              type="button"
              role="radio"
              aria-checked={checked}
              aria-label={STATUS_LABEL[st]}
              aria-disabled={!disabled && blocked ? true : undefined}
              title={STATUS_HINT[st]}
              tabIndex={i === stop ? 0 : -1}
              disabled={disabled}
              data-status={st}
              className={pending === st ? "is-pending" : undefined}
              onKeyDown={(e) => {
                onKeyDown(e, i);
              }}
              onClick={() => {
                if (blocked || st === value) return;
                onChange(st);
              }}
            >
              <span className="att-choice-mark" aria-hidden="true">
                {checked && <Icon name="check" className="size-4" />}
              </span>
              <span className="att-choice-word">{STATUS_WORD[st]}</span>
              {st !== "present" && (
                <span className="att-choice-code" aria-hidden="true">
                  {STATUS_CODE[st]}
                </span>
              )}
            </button>
          );
        })}
      </div>
      {pending && (
        <span className="sr-only" role="status">
          Đang lưu chấm công {name}…
        </span>
      )}
    </>
  );
}
