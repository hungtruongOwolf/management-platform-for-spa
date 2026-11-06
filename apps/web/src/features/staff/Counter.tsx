import { Icon } from "../../app/Icon";

/**
 * A 0–99 stepper (praise and complaints of a day): label, then one bordered group of
 * [− count +] with 44 px buttons. `disabled` = nothing may be written (closed month); `busy` = a
 * save is running: taps are ignored but the buttons keep focus (aria-disabled, not disabled), so a
 * keyboard user pressing + twice never loses their place.
 */
export function Counter({
  label,
  value,
  disabled,
  busy = false,
  onChange,
  name,
}: {
  label: string;
  value: number;
  disabled: boolean;
  busy?: boolean;
  onChange: (v: number) => void;
  /** Whose counter it is, for the accessible names of the buttons ("Thêm khen cho Lan"). */
  name?: string;
}) {
  const whose = name ? ` cho ${name}` : "";
  const step = (to: number) => {
    if (busy || to < 0 || to > 99) return;
    onChange(to);
  };
  return (
    <div className="counter" role="group" aria-label={`${label}${whose}`}>
      <span className="counter-label">{label}</span>
      <span className="counter-box">
        <button
          type="button"
          aria-label={`Bớt ${label.toLowerCase()}${whose}`}
          disabled={disabled || value === 0}
          aria-disabled={busy || undefined}
          onClick={() => {
            step(value - 1);
          }}
        >
          <Icon name="minus" className="size-4" />
        </button>
        <span className="counter-value tabular" aria-live="polite">
          {value}
        </span>
        <button
          type="button"
          aria-label={`Thêm ${label.toLowerCase()}${whose}`}
          disabled={disabled || value >= 99}
          aria-disabled={busy || undefined}
          onClick={() => {
            step(value + 1);
          }}
        >
          <Icon name="plus" className="size-4" />
        </button>
      </span>
    </div>
  );
}
