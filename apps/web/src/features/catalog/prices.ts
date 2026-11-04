import type { ServicePrices } from "@vpyd/api-client";
import { formatVnd } from "@vpyd/ui";

export interface PriceLine {
  label: "Nữ" | "Nam" | null;
  amount: string;
}

/** Flat price → one line; gender prices → Nữ / Nam lines, "—" where the gender is not offered. */
export function describePrices(prices: ServicePrices): PriceLine[] {
  if (prices.all != null) return [{ label: null, amount: formatVnd(prices.all) }];
  if (prices.female == null && prices.male == null) return [{ label: null, amount: "Chưa có giá" }];
  return [
    { label: "Nữ", amount: prices.female != null ? formatVnd(prices.female) : "—" },
    { label: "Nam", amount: prices.male != null ? formatVnd(prices.male) : "—" },
  ];
}

export function describeDuration(minutes: number | null | undefined): string {
  return minutes == null ? "Theo tư vấn" : `${String(minutes)} phút`;
}
