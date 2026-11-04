import type { Service, ServiceCategory } from "@vpyd/api-client";
import { queryOptions } from "@tanstack/react-query";
import { api } from "../../lib/api";

// Response shapes come from the generated OpenAPI contract (packages/api-client).
export const categoriesQuery = queryOptions({
  queryKey: ["catalog", "categories"],
  queryFn: async () => (await api("/service-categories")) as ServiceCategory[],
  staleTime: 5 * 60_000,
});

export const servicesQuery = queryOptions({
  queryKey: ["catalog", "services", { active: true }],
  queryFn: async () => (await api("/services")) as Service[],
  staleTime: 5 * 60_000,
});
