import type { Appointment } from "@vpyd/api-client";
import type { QueryClient } from "@tanstack/react-query";
import { categoriesQuery, servicesQuery } from "../catalog/api";
import { customerPackagesQuery } from "../sales/api";
import { type CartLine, byUseOrder, inCart, isUsable, priceFor } from "./api";

/**
 * The cart for completing a booked appointment: each booked service as a package deduction when
 * the customer has a usable package for it (same order as the desk, AC4.3), else as a one-off at
 * the listed price for the customer's gender; the booked technician on every line. Services that
 * need a gender the customer has not recorded are left for the desk to add. Keys are provisional:
 * the cart renumbers them. Lives in the calendar's chunk (not on the startup path).
 */
export async function cartFromAppointment(
  queryClient: QueryClient,
  a: Appointment,
): Promise<CartLine[]> {
  if (!a.customer_id) return [];
  const [pkgs, services, cats] = await Promise.all([
    queryClient.query(customerPackagesQuery(a.customer_id)),
    queryClient.query(servicesQuery(false)),
    queryClient.query(categoriesQuery),
  ]);
  const lines: CartLine[] = [];
  const usable = byUseOrder(pkgs).filter(isUsable);
  const staffId = a.staff_id ?? "";
  for (const s of a.services) {
    const left = (i: {
      id: string;
      service_id: string;
      sessions_remaining: number;
    }) =>
      i.service_id === s.service_id &&
      i.sessions_remaining - inCart(lines, i.id) > 0;
    const pkg = usable.find((p) => p.items.some(left));
    const item = pkg?.items.find(left);
    if (pkg && item) {
      lines.push({
        key: lines.length,
        source: "package",
        serviceId: s.service_id,
        serviceName: item.name,
        categoryName: item.category_name,
        itemId: item.id,
        packageName: pkg.name,
        staffId,
      });
      continue;
    }
    const service = services.find((x) => x.id === s.service_id);
    const price = service ? priceFor(service, a.customer_gender ?? null) : null;
    if (service && price?.ok)
      lines.push({
        key: lines.length,
        source: "one_off",
        serviceId: service.id,
        serviceName: service.name,
        categoryName:
          cats.find((c) => c.id === service.category_id)?.name ??
          s.category_name,
        priceVariant: null,
        price: price.price,
        staffId,
      });
  }
  return lines;
}
