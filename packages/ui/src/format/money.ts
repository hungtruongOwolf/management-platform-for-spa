const vndFormatter = new Intl.NumberFormat("vi-VN", { maximumFractionDigits: 0 });

/** Formats an integer amount of Vietnamese đồng, e.g. 1500000 → "1.500.000đ". */
export function formatVnd(amount: number): string {
  if (!Number.isSafeInteger(amount)) {
    throw new RangeError(`formatVnd expects a safe integer amount of đồng, got ${amount}`);
  }
  return `${vndFormatter.format(amount)}đ`;
}
