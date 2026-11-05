/**
 * "Thư Giãn · Gội đầu dưỡng sinh": a service with its menu category in front (EM round 3), so two
 * services of similar names read apart. The category comes from the API (`category_name`); without
 * it the name stands alone. Repeated words are not doubled ("Gội đầu · Gội đầu" never shows).
 */
/**
 * Expiry of a customer package. No `expires_on` means it never expires (spec R1-1 D2): it reads
 * "Không hạn" everywhere and never counts as expiring or expired (EM round 3, item 3).
 */
export function expiryText(expiresOn: string | null | undefined, format: (iso: string) => string): string {
  return expiresOn ? `HSD ${format(expiresOn)}` : NO_EXPIRY;
}

export const NO_EXPIRY = "Không hạn";

export function serviceLabel(name: string, category?: string | null): string {
  const c = category?.trim();
  if (!c || c.toLocaleLowerCase("vi") === name.trim().toLocaleLowerCase("vi")) return name;
  return `${c} · ${name}`;
}
