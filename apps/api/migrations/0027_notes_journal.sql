-- 0027: "Nhật ký ghi chú" (spec management-views §10, EM 2026-11-07): one read-back screen over
-- every note written in the app, newest first, by date range (default the last 30 days).
--
-- No new table and no copy of any note: the endpoint unions the sources where they live, each
-- under its own permission. These partial indexes keep each branch a short range scan over the
-- rows that carry a note (most visits, customers and voids have none). The attendance, daily
-- review and bonus/penalty books already have (tenant_id, work_date) indexes (0017).

CREATE INDEX visit_note_day_idx ON app.visit (tenant_id, business_date)
  WHERE note_ct IS NOT NULL;
CREATE INDEX visit_voided_at_idx ON app.visit (tenant_id, voided_at)
  WHERE status = 'voided';
CREATE INDEX sale_voided_at_idx ON app.sale (tenant_id, voided_at)
  WHERE status = 'voided';
CREATE INDEX customer_package_cancelled_at_idx ON app.customer_package (tenant_id, cancelled_at)
  WHERE cancelled_at IS NOT NULL;
CREATE INDEX customer_note_updated_idx ON app.customer (tenant_id, updated_at)
  WHERE note IS NOT NULL OR health_note_ct IS NOT NULL;
CREATE INDEX customer_follow_up_day_idx ON app.customer_follow_up (tenant_id, contacted_on)
  WHERE note IS NOT NULL;
CREATE INDEX staff_employment_ended_idx ON app.staff_employment (tenant_id, ended_on)
  WHERE end_reason IS NOT NULL;
