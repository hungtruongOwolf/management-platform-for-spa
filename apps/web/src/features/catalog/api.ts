import type { CategoryCreate, PackagePlan, PlanWrite, Service, ServiceCategory, ServiceWrite } from "@vpyd/api-client";
import { queryOptions } from "@tanstack/react-query";
import { api, ifMatch } from "../../lib/api";

// Response shapes come from the generated OpenAPI contract (packages/api-client).
export const categoriesQuery = queryOptions({
  queryKey: ["catalog", "categories"],
  queryFn: async () => (await api("/service-categories")) as ServiceCategory[],
  staleTime: 5 * 60_000,
});

/** Everything (including inactive) for owners; the counter will use `?active=true` (default). */
export function servicesQuery(includeInactive: boolean) {
  return queryOptions({
    queryKey: ["catalog", "services", { includeInactive }],
    queryFn: async () => (await api(includeInactive ? "/services?active=false" : "/services")) as Service[],
    staleTime: 5 * 60_000,
  });
}

export function plansQuery(includeInactive: boolean) {
  return queryOptions({
    queryKey: ["catalog", "plans", { includeInactive }],
    queryFn: async () => (await api(includeInactive ? "/package-plans?active=false" : "/package-plans")) as PackagePlan[],
    staleTime: 5 * 60_000,
  });
}

export async function createCategory(body: CategoryCreate): Promise<ServiceCategory> {
  return (await api("/service-categories", { method: "POST", body })) as ServiceCategory;
}

/** Create when `existing` is undefined, otherwise full replace guarded by its version. */
export async function saveService(body: ServiceWrite, existing?: Pick<Service, "id" | "version">): Promise<Service> {
  if (!existing) return (await api("/services", { method: "POST", body })) as Service;
  return (await api(`/services/${existing.id}`, { method: "PUT", body, headers: ifMatch(existing.version) })) as Service;
}

export async function savePlan(body: PlanWrite, existing?: Pick<PackagePlan, "id" | "version">): Promise<PackagePlan> {
  if (!existing) return (await api("/package-plans", { method: "POST", body })) as PackagePlan;
  return (await api(`/package-plans/${existing.id}`, { method: "PUT", body, headers: ifMatch(existing.version) })) as PackagePlan;
}
