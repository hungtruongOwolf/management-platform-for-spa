import type { FieldTexts } from "../../lib/errors";

/**
 * Field refusals only the catalog forms can get (name taken, mixed prices, a service twice in a
 * plan), passed as `fieldMessage(…, CATALOG_FIELDS)`. They live with those lazy screens, not in the
 * startup chunk (frontend.md "Perf budget").
 */
export const CATALOG_FIELDS: FieldTexts = {
  taken: "Tên này đã được dùng.",
  mixed_variants: "Chọn một giá chung hoặc giá theo giới tính, không chọn cả hai.",
  duplicate: "Dịch vụ bị trùng trong gói.",
};
