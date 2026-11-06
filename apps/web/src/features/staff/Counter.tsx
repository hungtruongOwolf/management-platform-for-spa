/** A 0–99 counter with − / + buttons (praise and complaints of a day). */
export function Counter({
  label,
  value,
  disabled,
  onChange,
  name,
}: {
  label: string;
  value: number;
  disabled: boolean;
  onChange: (v: number) => void;
  /** Whose counter it is, for the accessible names of the buttons ("Thêm khen cho Lan"). */
  name?: string;
}) {
  const whose = name ? ` cho ${name}` : "";
  return (
    <div className="counter" role="group" aria-label={`${label}${whose}`}>
      <span className="counter-label">{label}</span>
      <button
        type="button"
        className="icon-btn"
        aria-label={`Bớt ${label.toLowerCase()}${whose}`}
        disabled={disabled || value === 0}
        onClick={() => {
          onChange(value - 1);
        }}
      >
        −
      </button>
      <span className="counter-value tabular" aria-live="polite">
        {value}
      </span>
      <button
        type="button"
        className="icon-btn"
        aria-label={`Thêm ${label.toLowerCase()}${whose}`}
        disabled={disabled || value >= 99}
        onClick={() => {
          onChange(value + 1);
        }}
      >
        +
      </button>
    </div>
  );
}
