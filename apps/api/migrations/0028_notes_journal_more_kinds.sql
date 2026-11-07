-- 0028: "Nhật ký ghi chú" gets five more kinds (EM Q-N1 2026-11-07, spec management-views §10):
-- booking notes, monthly evaluation comments / coaching plans, staff HR notes, package-extension
-- reasons and discount reasons. As in 0027, nothing is copied: only partial indexes so each new
-- branch of `GET /notes` is a short range scan over the rows that carry text.

CREATE INDEX appointment_note_created_idx ON app.appointment (tenant_id, created_at)
  WHERE note IS NOT NULL;
CREATE INDEX staff_evaluation_text_updated_idx ON app.staff_evaluation (tenant_id, updated_at)
  WHERE comment IS NOT NULL OR coaching_plan IS NOT NULL;
CREATE INDEX staff_note_updated_idx ON app.staff (tenant_id, updated_at)
  WHERE note IS NOT NULL;
CREATE INDEX customer_package_extension_day_idx ON app.customer_package_extension (tenant_id, business_date);
CREATE INDEX sale_discount_day_idx ON app.sale (tenant_id, business_date)
  WHERE discount_reason IS NOT NULL;
